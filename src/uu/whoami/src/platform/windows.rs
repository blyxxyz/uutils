/*
 * This file is part of the uutils coreutils package.
 *
 * (c) Jordi Boggiano <j.boggiano@seld.be>
 *
 * For the full copyright and license information, please view the LICENSE
 * file that was distributed with this source code.
 */

extern crate winapi;

use self::winapi::shared::lmcons;
use self::winapi::shared::minwindef::DWORD;
use self::winapi::um::winbase;
use std::io::{Error, Result};
use uucore::wide::FromWide;

pub fn get_username() -> Result<String> {
    const BUF_LEN: DWORD = lmcons::UNLEN + 1;
    let mut buffer = [0_u16; BUF_LEN as usize];
    let mut len = BUF_LEN;
    // SAFETY: buffer.len() == len
    unsafe {
        if winbase::GetUserNameW(buffer.as_mut_ptr(), &mut len) == 0 {
            return Err(Error::last_os_error());
        }
    }
    let username = String::from_wide(&buffer[..len as usize - 1]);
    Ok(username)
}
