use libc::{
    c_char, c_int, c_void, size_t, MAP_FAILED, MAP_SHARED, MS_SYNC, O_RDWR, PROT_READ, PROT_WRITE,
};
use std::ffi::CStr;
use std::ptr;

#[no_mangle]
pub extern "C" fn select_rv_shm_env(out_id: *mut *const c_char, out_size: *mut *const c_char) {
    unsafe {
        let mut a = libc::getenv("__DIFFRAY_RV_SHM_ID_A\0".as_ptr() as *const c_char);
        let mut s = libc::getenv("__DIFFRAY_RV_SHM_ID_A_SIZE\0".as_ptr() as *const c_char);
        if a.is_null() {
            a = libc::getenv("__DIFFRAY_TMP_RV_A\0".as_ptr() as *const c_char);
        }
        if s.is_null() {
            s = libc::getenv("__DIFFRAY_TMP_RV_A_SIZE\0".as_ptr() as *const c_char);
        }
        if !a.is_null() && !s.is_null() {
            if !out_id.is_null() {
                ptr::write(out_id, a);
            }
            if !out_size.is_null() {
                ptr::write(out_size, s);
            }
            return;
        }
        let mut b = libc::getenv("__DIFFRAY_RV_SHM_ID_B\0".as_ptr() as *const c_char);
        let mut bs = libc::getenv("__DIFFRAY_RV_SHM_ID_B_SIZE\0".as_ptr() as *const c_char);
        if b.is_null() {
            b = libc::getenv("__DIFFRAY_TMP_RV_B\0".as_ptr() as *const c_char);
        }
        if bs.is_null() {
            bs = libc::getenv("__DIFFRAY_TMP_RV_B_SIZE\0".as_ptr() as *const c_char);
        }
        if !out_id.is_null() {
            ptr::write(out_id, b);
        }
        if !out_size.is_null() {
            ptr::write(out_size, bs);
        }
    }
}

struct MapInfo {
    ptr: *mut c_void,
    fd: c_int,
    from_shmat: bool,
    len: usize,
}

unsafe fn try_map_posix(name: *const c_char, len: usize) -> MapInfo {
    if name.is_null() {
        return MapInfo {
            ptr: ptr::null_mut(),
            fd: -1,
            from_shmat: false,
            len,
        };
    }
    let fd = libc::shm_open(name, O_RDWR, 0o666);
    if fd >= 0 {
        let map = libc::mmap(
            ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_SHARED,
            fd,
            0,
        );
        if map != MAP_FAILED {
            // leave fd for caller to close after munmap
            return MapInfo {
                ptr: map,
                fd,
                from_shmat: false,
                len,
            };
        }
        libc::close(fd);
    }
    MapInfo {
        ptr: ptr::null_mut(),
        fd: -1,
        from_shmat: false,
        len,
    }
}

unsafe fn try_map_numeric(id: *const c_char, len: usize) -> MapInfo {
    if id.is_null() {
        return MapInfo {
            ptr: ptr::null_mut(),
            fd: -1,
            from_shmat: false,
            len,
        };
    }
    let c = CStr::from_ptr(id);
    if let Ok(s) = c.to_str() {
        if s.starts_with("sysv:") {
            if let Ok(n) = s[5..].parse::<c_int>() {
                let p = libc::shmat(n, ptr::null(), 0);
                if p != (-(1 as isize)) as *mut c_void {
                    return MapInfo {
                        ptr: p,
                        fd: -1,
                        from_shmat: true,
                        len,
                    };
                }
            }
            return MapInfo {
                ptr: ptr::null_mut(),
                fd: -1,
                from_shmat: false,
                len,
            };
        }
        if let Ok(n) = s.parse::<c_int>() {
            // try mmap(fd)
            let map = libc::mmap(
                ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                n,
                0,
            );
            if map != MAP_FAILED {
                return MapInfo {
                    ptr: map,
                    fd: n,
                    from_shmat: false,
                    len,
                };
            }
            // try shmat
            let p = libc::shmat(n, ptr::null(), 0);
            if p != (-(1 as isize)) as *mut c_void {
                return MapInfo {
                    ptr: p,
                    fd: -1,
                    from_shmat: true,
                    len,
                };
            }
        }
    }
    MapInfo {
        ptr: ptr::null_mut(),
        fd: -1,
        from_shmat: false,
        len,
    }
}

#[no_mangle]
pub extern "C" fn write_return_value_to_shm(
    env_id: *const c_char,
    env_size: *const c_char,
    result: i32,
) {
    unsafe {
        if env_id.is_null() || env_size.is_null() {
            return;
        }
        let size_c = CStr::from_ptr(env_size);
        let size = if let Ok(s) = size_c.to_str() {
            s.parse::<usize>().ok()
        } else {
            None
        };
        let len = match size {
            Some(v) if v >= std::mem::size_of::<i32>() => v,
            _ => return,
        };

        // try numeric/sysv first
        let mut info = try_map_numeric(env_id, len);
        if info.ptr.is_null() {
            info = try_map_posix(env_id, len);
        }
        if !info.ptr.is_null() {
            let p = info.ptr as *mut i32;
            ptr::write_volatile(p, result);
            libc::msync(
                p as *mut c_void,
                std::mem::size_of::<i32>() as size_t,
                MS_SYNC,
            );
            if info.from_shmat {
                // detach
                libc::shmdt(info.ptr);
            } else {
                // unmap and close fd if present
                let _ = libc::munmap(info.ptr, info.len);
                if info.fd >= 0 {
                    libc::close(info.fd);
                }
            }
        }
    }
}
