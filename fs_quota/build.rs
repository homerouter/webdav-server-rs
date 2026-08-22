extern crate cc;

use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::process::Command;

fn run_rpcgen() -> bool {
    let res_c = match Command::new("rpcgen").arg("-c").arg("src/rquota.x").output() {
        Ok(output) if output.status.success() => output,
        _ => {
            println!("cargo:warning=rpcgen unavailable or failed, skipping NFS quota RPC generation");
            return false;
        },
    };
    let csrc = String::from_utf8_lossy(&res_c.stdout);
    if let Ok(mut f) = File::create("src/rquota_xdr.c") {
        let _ = f.write_all(
            csrc.replace("/usr/include/rpcsvc/rquota.h", "./rquota.h")
                .replace("src/rquota.h", "./rquota.h")
                .as_bytes(),
        );
    }

    if let Ok(res_h) = Command::new("rpcgen").arg("-h").arg("src/rquota.x").output() {
        if res_h.status.success() {
            let hdr = String::from_utf8_lossy(&res_h.stdout);
            if let Ok(mut f) = File::create("src/rquota.h") {
                let _ = f.write_all(hdr.as_bytes());
            }
        }
    }
    true
}

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_nfs_c)");
    #[allow(unused_mut)]
    let mut builder = cc::Build::new();

    #[cfg(target_os = "linux")]
    builder.file("src/quota-linux.c");

    #[cfg(target_os = "linux")]
    #[allow(unused_mut)]
    let mut has_c_files = true;
    #[cfg(not(target_os = "linux"))]
    #[allow(unused_mut)]
    let mut has_c_files = false;

    #[cfg(feature = "nfs")]
    let has_nfs =
        run_rpcgen() || (Path::new("src/rquota_xdr.c").exists() && Path::new("src/rquota.h").exists());
    #[cfg(not(feature = "nfs"))]
    let has_nfs = false;

    if has_nfs {
        if Path::new("/usr/include/tirpc").exists() {
            // Fedora does not include RPC support in glibc anymore, so use tirpc instead.
            builder.include("/usr/include/tirpc");
        }
        builder.file("src/quota-nfs.c").file("src/rquota_xdr.c");
        has_c_files = true;
        println!("cargo:rustc-cfg=has_nfs_c");
    }

    if has_c_files {
        builder
            .flag_if_supported("-Wno-unused-variable")
            .compile("fs_quota");
    }

    if has_nfs {
        if Path::new("/usr/include/tirpc").exists() {
            println!("cargo:rustc-link-lib=tirpc");
        } else {
            println!("cargo:rustc-link-lib=rpcsvc");
        }
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    println!("cargo:rerun-if-changed=src/quota-linux.c");

    #[cfg(feature = "nfs")]
    {
        println!("cargo:rerun-if-changed=src/rquota.x");
        println!("cargo:rerun-if-changed=src/quota-nfs.c");
    }
}
