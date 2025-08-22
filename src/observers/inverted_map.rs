use core::ptr;
use libafl::observers::{DifferentialObserver, Observer};
use libafl::Error;
use libafl_bolts::Named;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

fn default_src() -> *const u8 {
    ptr::null()
}

fn default_dst() -> *mut u8 {
    ptr::null_mut()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct InvertedCopyUpdater {
    // don't serialize nor deserialize the raw pointers; provide defaults on deserialization
    #[serde(skip, default = "default_src")]
    src: *const u8,
    #[serde(skip, default = "default_dst")]
    dst: *mut u8,
    len: usize,
}

impl InvertedCopyUpdater {
    pub fn new(src: *const u8, dst: *mut u8, len: usize) -> Self {
        Self { src, dst, len }
    }
}

impl Named for InvertedCopyUpdater {
    fn name(&self) -> &Cow<'static, str> {
        static NAME: Cow<'static, str> = Cow::Borrowed("inverted-copy-updater");
        &NAME
    }
}

impl<OTA, OTB, I, S> DifferentialObserver<OTA, OTB, I, S> for InvertedCopyUpdater {
    fn pre_observe_first(&mut self, _observers: &mut OTA) -> Result<(), Error> {
        Ok(())
    }

    fn post_observe_first(&mut self, _observers: &mut OTA) -> Result<(), Error> {
        Ok(())
    }

    fn pre_observe_second(&mut self, _observers: &mut OTB) -> Result<(), Error> {
        Ok(())
    }

    fn post_observe_second(&mut self, _observers: &mut OTB) -> Result<(), Error> {
        unsafe {
            // Copy and invert byte-by-byte
            for i in 0..self.len {
                let v = ptr::read(self.src.add(i));
                ptr::write(self.dst.add(i), 255u8.wrapping_sub(v));
            }
        }
        Ok(())
    }
}

impl<I, S> Observer<I, S> for InvertedCopyUpdater {
    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
    fn pre_exec(&mut self, _state: &mut S, _input: &I) -> Result<(), Error> {
        Ok(())
    }
}
