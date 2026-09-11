//
// Linux specific systemcalls for quota.
//
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::{FqError, FsQuota, Mtab};

// The actual implementation is done in C, and imported here.
extern "C" {
    fn fs_quota_linux(
        device: *const c_char,
        id: c_int,
        do_group: c_int,
        bytes_used: *mut u64,
        bytes_limit: *mut u64,
        files_used: *mut u64,
        files_limit: *mut u64,
    ) -> c_int;
}

// wrapper for the C functions.
pub(crate) fn get_quota(device: impl AsRef<Path>, uid: u32) -> Result<FsQuota, FqError> {
    let id = uid as c_int;
    let device = device.as_ref();

    let mut bytes_used = 0u64;
    let mut bytes_limit = 0u64;
    let mut files_used = 0u64;
    let mut files_limit = 0u64;

    let path = CString::new(device.as_os_str().as_bytes())?;
    let rc = unsafe {
        fs_quota_linux(
            path.as_ptr(),
            id,
            0,
            &mut bytes_used as *mut u64,
            &mut bytes_limit as *mut u64,
            &mut files_used as *mut u64,
            &mut files_limit as *mut u64,
        )
    };

    // Error mapping.
    match rc {
        0 => {
            let m = |v| if v == 0xffffffffffffffff { None } else { Some(v) };
            Ok(FsQuota {
                bytes_used,
                bytes_limit: m(bytes_limit),
                files_used,
                files_limit: m(files_limit),
            })
        },
        1 => Err(FqError::NoQuota),
        _ => Err(FqError::IoError(io::Error::last_os_error())),
    }
}

// read /etc/mtab.
pub(crate) fn read_mtab() -> io::Result<Vec<Mtab>> {
    let f = File::open("/etc/mtab")?;
    let reader = BufReader::new(f);
    let mut result = Vec::new();
    for l in reader.lines() {
        let l2 = l?;
        if let Some(entry) = parse_mtab_line(l2.trim()) {
            result.push(entry);
        }
    }
    Ok(result)
}

fn parse_mtab_line(line: &str) -> Option<Mtab> {
    let words = line.split_whitespace().collect::<Vec<_>>();
    if line.is_empty() || line.starts_with('#') || words.len() < 3 {
        return None;
    }
    let (host, device) = if words[2].starts_with("nfs") {
        let (host, path) = words[0].split_once(':')?;
        (Some(host.to_string()), path)
    } else {
        (None, words[0])
    };
    Some(Mtab {
        host,
        device: device.to_string(),
        directory: words[1].to_string(),
        fstype: words[2].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_mtab_entry_uses_device_field() {
        let entry = parse_mtab_line("/dev/vda1 / ext4 rw 0 0").unwrap();
        assert_eq!(entry.device, "/dev/vda1");
        assert_eq!(entry.fstype, "ext4");
    }
}
