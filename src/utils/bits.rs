#![allow(dead_code)]
// Utilities for packed bit operations (one bit per index) used by feedbacks.
//
// This module provides two sets of helpers:
// 1. Small, zero-dependency helpers that operate on `Vec<u64>` / `&[u64]`.
//    These are appropriate for single-threaded scenarios and mirror the
//    project's existing storage format for seen-first bits.
// 2. Atomic helpers that operate on slices of `AtomicU64` and provide
//    lock-free test-and-set semantics for concurrent writers. These helpers
//    do NOT perform resizing: the caller must ensure the backing slice has
//    sufficient length (to avoid data races on resizing).

use std::sync::atomic::{AtomicU64, Ordering};

/// Borrowing view over a slice of u64 words for read-only bit access.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct BitsRef<'a> {
    words: &'a [u64],
}

impl<'a> BitsRef<'a> {
    pub fn new(words: &'a [u64]) -> Self {
        Self { words }
    }

    /// Read bit at `idx`. Out-of-range reads return false.
    #[inline]
    pub fn get(&self, idx: usize) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        if word < self.words.len() {
            ((self.words[word] >> bit) & 1) == 1
        } else {
            false
        }
    }

    /// Number of u64 words in the backing slice.
    pub fn len(&self) -> usize {
        self.words.len()
    }
}

/// Owned, growable bitset backed by `Vec<u64>`.
#[allow(dead_code)]
#[derive(Debug, Default, Clone)]
pub struct Bits {
    words: Vec<u64>,
}

impl Bits {
    /// Create an empty bitset.
    pub fn new() -> Self {
        Self { words: Vec::new() }
    }

    /// Create a bitset preallocated for at least `capacity_bits` bits.
    pub fn with_capacity_bits(capacity_bits: usize) -> Self {
        let words = vec![0u64; (capacity_bits + 63) / 64];
        Self { words }
    }

    /// Read bit at `idx`. Out-of-range reads return false.
    #[inline]
    pub fn get(&self, idx: usize) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        if word < self.words.len() {
            ((self.words[word] >> bit) & 1) == 1
        } else {
            false
        }
    }

    /// Ensure there is storage for the word addressing `idx`.
    #[inline]
    fn ensure_word_for(&mut self, idx: usize) {
        let word = idx >> 6;
        if self.words.len() <= word {
            self.words.resize(word + 1, 0);
        }
    }

    /// Set bit at `idx`. May resize the backing storage.
    #[inline]
    pub fn set(&mut self, idx: usize) {
        let word = idx >> 6;
        let bit = idx & 63;
        let mask = 1u64 << bit;

        // Fast path: word within current length
        if word < self.words.len() {
            self.words[word] |= mask;
            return;
        }

        // Slow path: need to resize then set
        self.words.resize(word + 1, 0);
        self.words[word] |= mask;
    }

    /// Test whether the bit was already set, and set it if not. Returns true
    /// if previously set, false if we set it now.
    #[inline]
    pub fn test_and_set(&mut self, idx: usize) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        let mask = 1u64 << bit;

        // Fast path: word within current length
        if word < self.words.len() {
            let prev = (self.words[word] & mask) != 0;
            if !prev {
                self.words[word] |= mask;
            }
            return prev;
        }

        // Slow path: need to resize and set (previously unset)
        self.words.resize(word + 1, 0);
        self.words[word] |= mask;
        false
    }

    /// Expose underlying words slice for read-only usage.
    pub fn as_slice(&self) -> &[u64] {
        &self.words
    }

    /// Expose underlying words vector for mutation or persistence.
    pub fn into_vec(self) -> Vec<u64> {
        self.words
    }

    /// Convert a Vec<u64> into a Bits wrapper without copying.
    pub fn from_vec(words: Vec<u64>) -> Self {
        Self { words }
    }
}

/// Atomic, non-resizing bitset backed by `Vec<AtomicU64>`.
#[derive(Debug)]
#[allow(dead_code)]
pub struct AtomicBits {
    words: Vec<AtomicU64>,
}

impl AtomicBits {
    /// Create an AtomicBits from a preallocated Vec<AtomicU64>.
    pub fn from_vec(words: Vec<AtomicU64>) -> Self {
        Self { words }
    }

    /// Create an AtomicBits with `n_words` zeroed words.
    pub fn with_words(n_words: usize) -> Self {
        let mut v = Vec::with_capacity(n_words);
        for _ in 0..n_words {
            v.push(AtomicU64::new(0));
        }
        Self { words: v }
    }

    /// Read bit at `idx` with the specified ordering.
    #[inline]
    pub fn get(&self, idx: usize, ordering: Ordering) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        if word < self.words.len() {
            let v = self.words[word].load(ordering);
            (v >> bit) & 1 != 0
        } else {
            false
        }
    }

    /// Atomically test-and-set the bit at `idx`. Returns previous value.
    #[inline]
    pub fn test_and_set(&self, idx: usize, ordering: Ordering) -> bool {
        let word = idx >> 6;
        let bit = idx & 63;
        if word >= self.words.len() {
            return false;
        }
        let mask = 1u64 << bit;
        let prev = self.words[word].fetch_or(mask, ordering);
        (prev & mask) != 0
    }

    /// Atomically set the bit at `idx`. Returns previous value.
    #[inline]
    pub fn set(&self, idx: usize, ordering: Ordering) -> bool {
        self.test_and_set(idx, ordering)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use std::sync::atomic::Ordering;

    #[test]
    fn test_seen_first_basic() {
        let mut b = Bits::new();
        // out-of-range reads are false
        assert!(!b.get(0));

        // test_and_set on fresh -> returns false (was not set), subsequent returns true
        assert!(!b.test_and_set(5));
        assert!(b.get(5));
        assert!(b.test_and_set(5));

        // set explicitly
        b.set(130);
        assert!(b.get(130));
    }

    #[test]
    fn test_atomic_test_and_set() {
        let n_words = 4usize; // covers 256 bits
        let words: Vec<AtomicU64> = (0..n_words).map(|_| AtomicU64::new(0)).collect();
        let ab = AtomicBits::from_vec(words);
        // index within range
        let idx = 10usize;
        assert!(!ab.get(idx, Ordering::SeqCst));
        assert!(!ab.test_and_set(idx, Ordering::SeqCst));
        assert!(ab.get(idx, Ordering::SeqCst));
        assert!(ab.test_and_set(idx, Ordering::SeqCst));

        // out-of-range index returns false (no resize)
        let idx2 = n_words * 64 + 1;
        assert!(!ab.get(idx2, Ordering::SeqCst));
        assert!(!ab.test_and_set(idx2, Ordering::SeqCst));
    }
}
