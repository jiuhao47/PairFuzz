use libc::{c_char, c_int, c_void, MAP_FAILED, MAP_SHARED, O_RDWR, PROT_READ, PROT_WRITE};
use std::ffi::CStr;
use std::ptr;
use std::sync::atomic::{AtomicI32, AtomicPtr, Ordering};

const UNINITIALIZED: i32 = 0;
const STANDALONE: i32 = 1;
const MAPPED: i32 = 2;
const FAILED: i32 = -1;
const RESULT_SIZE: usize = std::mem::size_of::<u64>();

static RESULT_STATE: AtomicI32 = AtomicI32::new(UNINITIALIZED);
static RESULT_PTR: AtomicPtr<u64> = AtomicPtr::new(ptr::null_mut());

unsafe fn environment(name: &'static [u8]) -> *const c_char {
    libc::getenv(name.as_ptr().cast())
}

unsafe fn clear_former_environment() {
    let prefix = [b"__DIFF".as_slice(), b"RAY_".as_slice()].concat();
    for suffix in [
        b"RV_ROLE".as_slice(),
        b"RV_SHM_ID_A".as_slice(),
        b"RV_SHM_ID_A_SIZE".as_slice(),
        b"RV_SHM_ID_B".as_slice(),
        b"RV_SHM_ID_B_SIZE".as_slice(),
        b"TMP_RV_A".as_slice(),
        b"TMP_RV_A_SIZE".as_slice(),
        b"TMP_RV_B".as_slice(),
        b"TMP_RV_B_SIZE".as_slice(),
    ] {
        let mut name = Vec::with_capacity(prefix.len() + suffix.len() + 1);
        name.extend_from_slice(&prefix);
        name.extend_from_slice(suffix);
        name.push(0);
        libc::unsetenv(name.as_ptr().cast());
    }
}

unsafe fn clear_opposite_environment(role: u8) {
    let names: &[&[u8]] = match role {
        b'A' => &[
            b"__PAIRFUZZ_RV_SHM_ID_B\0",
            b"__PAIRFUZZ_RV_SHM_ID_B_SIZE\0",
            b"__PAIRFUZZ_TMP_RV_B\0",
            b"__PAIRFUZZ_TMP_RV_B_SIZE\0",
        ],
        b'B' => &[
            b"__PAIRFUZZ_RV_SHM_ID_A\0",
            b"__PAIRFUZZ_RV_SHM_ID_A_SIZE\0",
            b"__PAIRFUZZ_TMP_RV_A\0",
            b"__PAIRFUZZ_TMP_RV_A_SIZE\0",
        ],
        _ => return,
    };
    for name in names {
        libc::unsetenv(name.as_ptr().cast());
    }
}

unsafe fn role_environment(
    role: u8,
) -> (*const c_char, *const c_char, *const c_char, *const c_char) {
    match role {
        b'A' => (
            environment(b"__PAIRFUZZ_RV_SHM_ID_A\0"),
            environment(b"__PAIRFUZZ_RV_SHM_ID_A_SIZE\0"),
            environment(b"__PAIRFUZZ_TMP_RV_A\0"),
            environment(b"__PAIRFUZZ_TMP_RV_A_SIZE\0"),
        ),
        b'B' => (
            environment(b"__PAIRFUZZ_RV_SHM_ID_B\0"),
            environment(b"__PAIRFUZZ_RV_SHM_ID_B_SIZE\0"),
            environment(b"__PAIRFUZZ_TMP_RV_B\0"),
            environment(b"__PAIRFUZZ_TMP_RV_B_SIZE\0"),
        ),
        _ => (ptr::null(), ptr::null(), ptr::null(), ptr::null()),
    }
}

unsafe fn selected_environment(role: u8) -> (*const c_char, *const c_char) {
    let (id, size, fallback_id, fallback_size) = role_environment(role);
    (
        if id.is_null() { fallback_id } else { id },
        if size.is_null() { fallback_size } else { size },
    )
}

unsafe fn any_result_environment_present() -> bool {
    [
        b"__PAIRFUZZ_RV_SHM_ID_A\0".as_slice(),
        b"__PAIRFUZZ_RV_SHM_ID_A_SIZE\0".as_slice(),
        b"__PAIRFUZZ_RV_SHM_ID_B\0".as_slice(),
        b"__PAIRFUZZ_RV_SHM_ID_B_SIZE\0".as_slice(),
        b"__PAIRFUZZ_TMP_RV_A\0".as_slice(),
        b"__PAIRFUZZ_TMP_RV_A_SIZE\0".as_slice(),
        b"__PAIRFUZZ_TMP_RV_B\0".as_slice(),
        b"__PAIRFUZZ_TMP_RV_B_SIZE\0".as_slice(),
    ]
    .iter()
    .any(|name| !libc::getenv(name.as_ptr().cast()).is_null())
}

unsafe fn parse_size(size: *const c_char) -> Option<usize> {
    if size.is_null() {
        return None;
    }
    CStr::from_ptr(size).to_str().ok()?.parse().ok()
}

unsafe fn map_result(id: *const c_char, len: usize) -> *mut u64 {
    if id.is_null() || len < RESULT_SIZE {
        return ptr::null_mut();
    }
    let Ok(identifier) = CStr::from_ptr(id).to_str() else {
        return ptr::null_mut();
    };

    if let Some(number) = identifier.strip_prefix("sysv:") {
        let Ok(shmid) = number.parse::<c_int>() else {
            return ptr::null_mut();
        };
        let mapped = libc::shmat(shmid, ptr::null(), 0);
        return if mapped == (-1_isize) as *mut c_void {
            ptr::null_mut()
        } else {
            mapped.cast()
        };
    }

    if let Ok(fd) = identifier.parse::<c_int>() {
        let mapped = libc::mmap(
            ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            0,
        );
        if mapped != MAP_FAILED {
            libc::close(fd);
            return mapped.cast();
        }
        let mapped = libc::shmat(fd, ptr::null(), 0);
        return if mapped == (-1_isize) as *mut c_void {
            ptr::null_mut()
        } else {
            mapped.cast()
        };
    }

    let fd = libc::shm_open(id, O_RDWR, 0o666);
    if fd < 0 {
        return ptr::null_mut();
    }
    let mapped = libc::mmap(
        ptr::null_mut(),
        len,
        PROT_READ | PROT_WRITE,
        MAP_SHARED,
        fd,
        0,
    );
    libc::close(fd);
    if mapped == MAP_FAILED {
        ptr::null_mut()
    } else {
        mapped.cast()
    }
}

/// Resolve the explicit A/B role and cache the semantic-result mapping.
/// A target without any PairFuzz result environment remains runnable standalone.
#[no_mangle]
pub extern "C" fn pairfuzz_result_init() -> c_int {
    let state = RESULT_STATE.load(Ordering::Acquire);
    if state != UNINITIALIZED {
        return if state == FAILED { -1 } else { 0 };
    }

    unsafe {
        clear_former_environment();
        let role_ptr = environment(b"__PAIRFUZZ_RV_ROLE\0");
        if role_ptr.is_null() {
            if any_result_environment_present() {
                RESULT_STATE.store(FAILED, Ordering::Release);
                return -1;
            }
            RESULT_STATE.store(STANDALONE, Ordering::Release);
            return 0;
        }
        let role = CStr::from_ptr(role_ptr).to_bytes();
        if role.len() != 1 || !matches!(role[0], b'A' | b'B') {
            RESULT_STATE.store(FAILED, Ordering::Release);
            return -1;
        }

        let (id, size) = selected_environment(role[0]);
        if id.is_null() || size.is_null() {
            RESULT_STATE.store(FAILED, Ordering::Release);
            return -1;
        }
        let Some(len) = parse_size(size).filter(|len| *len >= RESULT_SIZE) else {
            RESULT_STATE.store(FAILED, Ordering::Release);
            return -1;
        };
        let mapped = map_result(id, len);
        if mapped.is_null() {
            RESULT_STATE.store(FAILED, Ordering::Release);
            return -1;
        }
        RESULT_PTR.store(mapped, Ordering::Release);
        clear_opposite_environment(role[0]);
        RESULT_STATE.store(MAPPED, Ordering::Release);
        0
    }
}

/// Publish one semantic result. Initialization and mapping happen at most once;
/// the execution hot path is a single volatile 64-bit store.
#[no_mangle]
pub extern "C" fn pairfuzz_publish_result(value: u64) {
    if RESULT_STATE.load(Ordering::Acquire) == UNINITIALIZED && pairfuzz_result_init() != 0 {
        return;
    }
    let result = RESULT_PTR.load(Ordering::Acquire);
    if !result.is_null() {
        unsafe { ptr::write_volatile(result, value) };
    }
}
