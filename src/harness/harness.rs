use libafl::executors::ExitKind;
use libafl::inputs::{BytesInput, HasTargetBytes};
use libafl_bolts::AsSlice;
use std::os::unix::io::RawFd;

// libc is used for dup/dup2 to temporarily redirect fds
use libc::{close, dup, dup2};

// Use crate-level bindings and observers defined in main.rs modules
use crate::bindings::{diffRayTestOneInput_1, diffRayTestOneInput_2};
use crate::observers;

/// First harness: calls the first target implementation and converts return code to ExitKind
pub fn first_harness(input: &BytesInput) -> ExitKind {
    unsafe { std::env::set_var("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=false") };
    // Check HARNESS_DEBUG environment variable to decide whether to keep target stdout/stderr.
    // If HARNESS_DEBUG is set to "1" or "true" (case-insensitive), we will NOT redirect output.
    let harness_debug: bool = match std::env::var_os("HARNESS_DEBUG") {
        Some(val) => {
            let s = val.to_string_lossy().to_ascii_lowercase();
            s == "1" || s == "true"
        }
        None => false,
    };

    // Silence target stdout/stderr by redirecting them to /dev/null while the target runs,
    // unless HARNESS_DEBUG is enabled.
    let stdout_fd: RawFd = 1;
    let stderr_fd: RawFd = 2;
    let mut saved_stdout: RawFd = -1;
    let mut saved_stderr: RawFd = -1;
    if !harness_debug {
        // Save original fds
        saved_stdout = unsafe { dup(stdout_fd) };
        saved_stderr = unsafe { dup(stderr_fd) };

        if saved_stdout >= 0 && saved_stderr >= 0 {
            // open /dev/null and dup2 onto stdout/stderr
            let devnull =
                unsafe { libc::open(b"/dev/null\0".as_ptr() as *const i8, libc::O_WRONLY) };
            if devnull >= 0 {
                unsafe { dup2(devnull, stdout_fd) };
                unsafe { dup2(devnull, stderr_fd) };
                unsafe { close(devnull) };
            }
        }
    }

    let target = input.target_bytes();
    let buf = target.as_slice();
    let result = unsafe { diffRayTestOneInput_1(buf.as_ptr(), buf.len()) };

    // restore stdout/stderr if we redirected them
    if !harness_debug {
        if saved_stdout >= 0 {
            unsafe { dup2(saved_stdout, stdout_fd) };
            unsafe { close(saved_stdout) };
        }
        if saved_stderr >= 0 {
            unsafe { dup2(saved_stderr, stderr_fd) };
            unsafe { close(saved_stderr) };
        }
    }
    // store numeric return value to observer TLS
    // println!("first_harness result: {}", result);
    observers::set_last_return(result as i32);
    if result == 0 {
        ExitKind::Ok
    } else {
        ExitKind::Ok
    }
}

/// Second harness: calls the second target implementation and converts return code to ExitKind
pub fn second_harness(input: &BytesInput) -> ExitKind {
    unsafe { std::env::set_var("ASAN_OPTIONS", "abort_on_error=1:detect_leaks=false") };
    // Check debug env var as above
    let harness_debug: bool = match std::env::var_os("HARNESS_DEBUG") {
        Some(val) => {
            let s = val.to_string_lossy().to_ascii_lowercase();
            s == "1" || s == "true"
        }
        None => false,
    };

    // Silence stdout/stderr similar to first_harness unless debug enabled
    let stdout_fd: RawFd = 1;
    let stderr_fd: RawFd = 2;
    let mut saved_stdout: RawFd = -1;
    let mut saved_stderr: RawFd = -1;
    if !harness_debug {
        saved_stdout = unsafe { dup(stdout_fd) };
        saved_stderr = unsafe { dup(stderr_fd) };

        if saved_stdout >= 0 && saved_stderr >= 0 {
            let devnull =
                unsafe { libc::open(b"/dev/null\0".as_ptr() as *const i8, libc::O_WRONLY) };
            if devnull >= 0 {
                unsafe { dup2(devnull, stdout_fd) };
                unsafe { dup2(devnull, stderr_fd) };
                unsafe { close(devnull) };
            }
        }
    }

    let target = input.target_bytes();
    let buf = target.as_slice();
    let result = unsafe { diffRayTestOneInput_2(buf.as_ptr(), buf.len()) };

    if !harness_debug {
        if saved_stdout >= 0 {
            unsafe { dup2(saved_stdout, stdout_fd) };
            unsafe { close(saved_stdout) };
        }
        if saved_stderr >= 0 {
            unsafe { dup2(saved_stderr, stderr_fd) };
            unsafe { close(saved_stderr) };
        }
    }
    // store numeric return value to observer TLS
    // println!("second_harness result: {}", result);
    observers::set_last_return(result as i32);
    if result == 0 {
        ExitKind::Ok
    } else {
        ExitKind::Ok
    }
}
