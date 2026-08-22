use std::ffi::{CStr, OsStr};
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use tokio::task::block_in_place;

#[derive(Debug)]
#[allow(dead_code)]
pub struct User {
    pub name: String,
    pub passwd: String,
    pub gecos: String,
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
    pub dir: PathBuf,
    pub shell: PathBuf,
}

unsafe fn cptr_to_osstr<'a>(c: *const libc::c_char) -> &'a OsStr {
    if c.is_null() {
        OsStr::from_bytes(b"")
    } else {
        let bytes = CStr::from_ptr(c).to_bytes();
        OsStr::from_bytes(bytes)
    }
}

unsafe fn cptr_to_path<'a>(c: *const libc::c_char) -> &'a Path {
    Path::new(cptr_to_osstr(c))
}

unsafe fn to_user(pwd: &libc::passwd) -> User {
    // turn into (unsafe!) rust slices
    let cs_name = if pwd.pw_name.is_null() {
        ""
    } else {
        CStr::from_ptr(pwd.pw_name).to_str().unwrap_or("")
    };
    let cs_passwd = if pwd.pw_passwd.is_null() {
        ""
    } else {
        CStr::from_ptr(pwd.pw_passwd).to_str().unwrap_or("")
    };
    let cs_gecos = if pwd.pw_gecos.is_null() {
        ""
    } else {
        CStr::from_ptr(pwd.pw_gecos).to_str().unwrap_or("")
    };
    let cs_dir = cptr_to_path(pwd.pw_dir);
    let cs_shell = cptr_to_path(pwd.pw_shell);

    // then turn the slices into safe owned values.
    User {
        name: cs_name.to_string(),
        passwd: cs_passwd.to_string(),
        gecos: cs_gecos.to_string(),
        dir: cs_dir.to_path_buf(),
        shell: cs_shell.to_path_buf(),
        uid: pwd.pw_uid,
        gid: pwd.pw_gid,
        groups: Vec::new(),
    }
}

impl User {
    pub fn by_name(name: &str, with_groups: bool) -> Result<User, io::Error> {
        let mut buf = [0u8; 1024];
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();

        let cname = match std::ffi::CString::new(name) {
            Ok(un) => un,
            Err(_) => return Err(io::Error::from_raw_os_error(libc::ENOENT)),
        };
        let ret = unsafe {
            libc::getpwnam_r(
                cname.as_ptr(),
                &mut pwd as *mut _,
                buf.as_mut_ptr() as *mut _,
                buf.len() as libc::size_t,
                &mut result as *mut _,
            )
        };

        if ret != 0 {
            return Err(io::Error::from_raw_os_error(ret));
        }
        if result.is_null() {
            return Err(io::Error::from_raw_os_error(libc::ENOENT));
        }
        let mut user = unsafe { to_user(&pwd) };

        if with_groups {
            let mut groups_buf = vec![0 as libc::gid_t; 64];
            let mut ngroups = groups_buf.len() as libc::c_int;
            let mut ret = unsafe {
                libc::getgrouplist(
                    cname.as_ptr(),
                    user.gid as libc::gid_t,
                    groups_buf.as_mut_ptr(),
                    &mut ngroups,
                )
            };
            if ret < 0 && ngroups > 0 {
                groups_buf.resize(ngroups as usize, 0);
                ret = unsafe {
                    libc::getgrouplist(
                        cname.as_ptr(),
                        user.gid as libc::gid_t,
                        groups_buf.as_mut_ptr(),
                        &mut ngroups,
                    )
                };
            }
            if ret >= 0 && ngroups > 0 {
                groups_buf.truncate(ngroups as usize);
                user.groups = groups_buf;
            }
        }

        Ok(user)
    }

    /*
    pub fn by_uid(uid: u32) -> Result<User, io::Error> {
        let mut buf = [0; 1024];
        let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result: *mut libc::passwd = std::ptr::null_mut();

        let ret = unsafe {
            getpwuid_r(
                uid,
                &mut pwd as *mut _,
                buf.as_mut_ptr(),
                buf.len() as libc::size_t,
                &mut result as *mut _,
            )
        };
        if ret == 0 {
            if result.is_null() {
                return Err(io::Error::from_raw_os_error(libc::ENOENT));
            }
            let p = unsafe { to_user(&pwd) };
            Ok(p)
        } else {
            Err(io::Error::from_raw_os_error(ret))
        }
    }
    */

    pub async fn by_name_async(name: &str, with_groups: bool) -> Result<User, io::Error> {
        block_in_place(move || User::by_name(name, with_groups))
    }
}
