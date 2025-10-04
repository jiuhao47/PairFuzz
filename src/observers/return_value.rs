use crate::utils::logging::debug;
use core::fmt::Debug;
use libafl::{executors::ExitKind, observers::Observer, state::HasExecutions, Error};
use libafl_bolts::shmem::StdShMem;
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::Cell;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

// Thread-local storage for the last return value written by the harness
thread_local! {
    static LAST_RETURN: Cell<Option<i32>> = Cell::new(None);
}

/// Set the last return value for the current thread (to be called by the harness)
pub fn set_last_return(val: i32) {
    LAST_RETURN.with(|c| c.set(Some(val)));
}

/// Take (consume) the last return value for the current thread
/// Returns None if no value was set
pub fn take_last_return() -> Option<i32> {
    LAST_RETURN.with(|c| {
        let v = c.get();
        c.set(None);
        v
    })
}

/// An observer that tracks the return value (i32) of harness executions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReturnValueObserver {
    name: Cow<'static, str>,
    /// The last observed numeric return value (from the harness)
    last_value: Option<i32>,
    /// Also keep track of last ExitKind for convenience
    last_exit_kind: Option<ExitKind>,
    /// Optional shared memory region that the target writes its numeric return into.
    /// If present, `post_exec*` will try to read an i32 from the start of this mapping.
    #[serde(skip)]
    rv_shmem: Option<StdShMem>,
}

impl ReturnValueObserver {
    pub fn new(name: &'static str) -> Self {
        Self {
            name: Cow::Borrowed(name),
            last_value: None,
            last_exit_kind: None,
            rv_shmem: None,
        }
    }

    /// Construct an observer that will read return value from the provided shared memory.
    pub fn new_with_shmem(name: &'static str, shmem: StdShMem) -> Self {
        Self {
            name: Cow::Borrowed(name),
            last_value: None,
            last_exit_kind: None,
            rv_shmem: Some(shmem),
        }
    }

    /// Attach/replace the rv shmem mapping for this observer.
    pub fn set_shmem(&mut self, shmem: StdShMem) {
        self.rv_shmem = Some(shmem);
    }

    #[must_use]
    pub fn last_value(&self) -> Option<i32> {
        self.last_value
    }

    #[must_use]
    pub fn last_exit_kind(&self) -> Option<ExitKind> {
        self.last_exit_kind.clone()
    }

    /// Temporary debug log used only by this observer (best-effort append).
    /// 保留下来用于调试：写入 run/log/return_value_debug.log。
    pub fn append_debug_log(&self, exit_kind: &ExitKind, value: Option<i32>) {
        let path = PathBuf::from("run/log/return_value_debug.log");
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut line = String::new();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        line.push_str(&format!("{:.3} [{}] {:?}", ts, self.name, exit_kind));
        if let Some(v) = value {
            line.push_str(&format!(" val={}", v));
        }
        line.push('\n');

        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = f.write_all(line.as_bytes());
        }
    }

    /// Snapshot the first `n` bytes of the attached shmem (if any).
    /// Returns a Vec<u8> copy of the bytes.
    pub fn snapshot_bytes(&self, n: usize) -> Option<Vec<u8>> {
        if let Some(shm) = &self.rv_shmem {
            let len = usize::min(shm.len(), n);
            let slice = &shm[..len];
            return Some(slice.to_vec());
        }
        None
    }
}

/// Whether return-value observer debug logging is enabled via env var
fn rv_obs_debug_enabled() -> bool {
    static RV_OBS_DEBUG: OnceLock<bool> = OnceLock::new();
    *RV_OBS_DEBUG.get_or_init(|| match std::env::var("DIFFRAY_RV_OBS_DEBUG") {
        Ok(s) => {
            let low = s.to_lowercase();
            low == "1" || low == "true"
        }
        Err(_) => false,
    })
}

impl Named for ReturnValueObserver {
    fn name(&self) -> &Cow<'static, str> {
        &self.name
    }
}

impl<I, S> Observer<I, S> for ReturnValueObserver
where
    S: HasExecutions,
{
    fn pre_exec(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        Ok(())
    }

    fn post_exec(&mut self, _state: &mut S, _input: &I, exit_kind: &ExitKind) -> Result<(), Error> {
        // Record ExitKind
        self.last_exit_kind = Some(exit_kind.clone());

        // Try to take a numeric return value written by the harness (TLS)
        // let mut observed_val = None;
        // Prefer shmem if present (forkserver targets will write into shmem), else fall back to TLS
        if let Some(shm) = &mut self.rv_shmem {
            // read first 4 bytes as native-endian i32 if possible
            // Try to read an i32 from the start of the shared mapping using pointer helpers.
            if let Some(ptr) = libafl_bolts::shmem::ShMem::as_mut_ptr_of::<i32>(&mut *shm) {
                // # Safety: we checked size and ptr is valid for i32
                let v = unsafe { ptr.read_volatile() };
                self.last_value = Some(v);
            }
        } else if let Some(v) = take_last_return() {
            self.last_value = Some(v);
        }

        // Keep debug logging for diagnostics, controlled by DIFFRAY_RV_OBS_DEBUG
        if rv_obs_debug_enabled() {
            if let Some(v) = self.last_value {
                debug(
                    "return_value",
                    &format!("observed return value = {} ({:?})", v, exit_kind),
                );
            } else {
                debug(
                    "return_value",
                    &format!("no return value observed ({:?})", exit_kind),
                );
            }
        }

        Ok(())
    }

    fn pre_exec_child(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        Ok(())
    }

    fn post_exec_child(
        &mut self,
        _state: &mut S,
        _input: &I,
        exit_kind: &ExitKind,
    ) -> Result<(), Error> {
        self.last_exit_kind = Some(exit_kind.clone());
        // Prefer shmem if present (forkserver targets will write into shmem), else fall back to TLS
        if let Some(shm) = &mut self.rv_shmem {
            if let Some(ptr) = libafl_bolts::shmem::ShMem::as_mut_ptr_of::<i32>(&mut *shm) {
                let v = unsafe { ptr.read_volatile() };
                self.last_value = Some(v);
            }
        } else if let Some(v) = take_last_return() {
            self.last_value = Some(v);
        }
        // self.append_debug_log(exit_kind, observed_val);
        if rv_obs_debug_enabled() {
            if let Some(v) = self.last_value {
                debug(
                    "return_value",
                    &format!("observed return value (child) = {} ({:?})", v, exit_kind),
                );
            } else {
                debug(
                    "return_value",
                    &format!("no return value observed (child) ({:?})", exit_kind),
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libafl::executors::ExitKind;

    struct DummyState {
        executions: u64,
    }

    impl DummyState {
        fn new() -> Self {
            Self { executions: 0 }
        }
    }

    impl HasExecutions for DummyState {
        fn executions(&self) -> &u64 {
            &self.executions
        }
        fn executions_mut(&mut self) -> &mut u64 {
            &mut self.executions
        }
    }

    #[test]
    fn tls_set_and_take() {
        set_last_return(42);
        assert_eq!(take_last_return(), Some(42));
        assert_eq!(take_last_return(), None);
    }

    #[test]
    fn observer_records_value() {
        let mut obs = ReturnValueObserver::new("rv");
        set_last_return(7);
        let mut state = DummyState::new();
        obs.post_exec(&mut state, &(), &ExitKind::Ok).unwrap();
        assert_eq!(obs.last_value(), Some(7));
    }
}
