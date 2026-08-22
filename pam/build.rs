use std::path::Path;

fn main() {
    println!("cargo:rustc-check-cfg=cfg(has_pam_c)");
    let has_pam_h = Path::new("/usr/include/security/pam_appl.h").exists()
        || Path::new("/usr/local/include/security/pam_appl.h").exists()
        || Path::new("/data/data/com.termux/files/usr/include/security/pam_appl.h").exists();

    if has_pam_h {
        println!("cargo:rustc-link-lib=pam");
        cc::Build::new().file("src/pam.c").compile("rpam");
        println!("cargo:rustc-cfg=has_pam_c");
    } else {
        println!("cargo:warning=security/pam_appl.h not found, pam-sandboxed will be built without native PAM linkage");
    }
}
