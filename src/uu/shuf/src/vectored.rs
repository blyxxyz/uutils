//! A convenience layer for vectored writes. This allows you to write many byte
//! slices in one go, which can be significantly more efficient.
//!
//! The standard library has an API for this but the high-level write_all()
//! equivalent is still unstable so we roll our own.
//!
//! This is only available on Unix. Windows only supports vectored writes to
//! sockets, and Rust's stdlib imposes a maximum length of 4GB for IoSlices on
//! Windows. Windows code should not (and cannot) use this module.
//!
//! This module can be moved to uucore if it's useful for other utilities.

use std::{
    io::{Error, ErrorKind, IoSlice, Result, Write},
    sync::OnceLock,
};

/// The maximum number of IoSlices that can be written at once.
///
/// It's better not to save up every single slice before writing but rather
/// use this as a guide to batch them.
#[cfg(not(any(target_os = "redox", target_env = "newlib")))]
pub fn iov_max() -> usize {
    use libc::{sysconf, _SC_IOV_MAX};

    static IOV_MAX: OnceLock<usize> = OnceLock::new();

    *IOV_MAX.get_or_init(|| {
        // SAFETY: sysconf is safe to call.
        let ret = unsafe { sysconf(_SC_IOV_MAX) };
        if ret == -1 {
            // Something went wrong. This should be impossible.
            // Using a too low (or too high) value is acceptable, so only crash
            // if we're in a debugging scenario.
            if cfg!(debug_assertions) {
                panic!("sysconf(_SC_IOV_MAX): {:?}", Error::last_os_error());
            } else {
                // 16 is the minimum prescribed by POSIX.
                // Most modern systems use 1024.
                16
            }
        } else {
            ret.try_into().unwrap()
        }
    })
}

// Redox and Newlib are the only Unix targets in the libc crate that don't have
// sysconf.

// Redox currently emulates vectored writes inside relibc:
// https://gitlab.redox-os.org/redox-os/relibc/-/blob/4d2d062f0733a039a10e32375328eed47d651fe5/src/header/sys_uio/mod.rs#L59-70
// It has an IOV_MAX of 1024.
// Hopefully it will get kernel support some day.
#[cfg(target_os = "redox")]
pub fn iov_max() -> usize {
    1024
}

#[cfg(target_env = "newlib")]
pub fn iov_max() -> usize {
    // POSIX minimum.
    16
}

pub fn write_all_vectored<W: Write>(writer: &mut W, mut buffers: &[IoSlice<'_>]) -> Result<()> {
    while !buffers.is_empty() {
        // We should only try to write as many buffers at a time as the OS
        // can handle (this is indicated through IOV_MAX). If we try to write
        // all buffers at once the code will still work but it will run a lot
        // slower. Some layers down the stack check all buffers, even the ones
        // that don't get written.
        // See https://github.com/rust-lang/rust/pull/121938
        let batch = buffers.get(..iov_max()).unwrap_or(buffers);

        match writer.write_vectored(batch) {
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
            Ok(0) if batch.iter().any(|b| !b.is_empty()) => {
                return Err(Error::new(
                    ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            Ok(mut to_remove) => {
                while to_remove != 0 {
                    let head = buffers.first().expect("impossibly large write");
                    if head.len() <= to_remove {
                        to_remove -= head.len();
                    } else {
                        // Partial write that ended in the middle of a buffer. Write the
                        // rest of this buffer conventionally before continuing.
                        // To include the rest of this buffer in the next vectored write we'd
                        // have to mutate the IoSlice itself. And this case is rare, it's typical
                        // for the OS to write exactly IOV_MAX slices every time.
                        writer.write_all(&head[to_remove..])?;
                        to_remove = 0;
                    }
                    buffers = &buffers[1..];
                }
            }
        }
    }
    Ok(())
}
