//! Custom observers for the diffray fuzzer

pub mod return_value;

pub use return_value::{set_last_return, take_last_return, ReturnValueObserver};
