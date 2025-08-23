//! Custom observers for the diffray fuzzer
#![allow(dead_code)]

pub mod coverage_combiner;
pub mod coverage_helpers;
pub mod return_value;

// ComboCoverageBuilder is available via observers::coverage_combiner::ComboCoverageBuilder if needed
pub use return_value::{set_last_return, ReturnValueObserver};
