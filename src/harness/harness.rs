use libafl::executors::ExitKind;
use libafl::inputs::{BytesInput, HasTargetBytes};
use libafl_bolts::AsSlice;

// Use crate-level bindings and observers defined in main.rs modules
use crate::bindings::{diffRayTestOneInput_1, diffRayTestOneInput_2};
use crate::observers;

/// First harness: calls the first target implementation and converts return code to ExitKind
pub fn first_harness(input: &BytesInput) -> ExitKind {
    let target = input.target_bytes();
    let buf = target.as_slice();
    let result = unsafe { diffRayTestOneInput_1(buf.as_ptr(), buf.len()) };
    // store numeric return value to observer TLS
    observers::set_last_return(result as i32);
    if result == 0 {
        ExitKind::Ok
    } else {
        ExitKind::Crash
    }
}

/// Second harness: calls the second target implementation and converts return code to ExitKind
pub fn second_harness(input: &BytesInput) -> ExitKind {
    let target = input.target_bytes();
    let buf = target.as_slice();
    let result = unsafe { diffRayTestOneInput_2(buf.as_ptr(), buf.len()) };
    // store numeric return value to observer TLS
    observers::set_last_return(result as i32);
    if result == 0 {
        ExitKind::Ok
    } else {
        ExitKind::Crash
    }
}
