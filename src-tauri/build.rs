use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("awake_display");

    let status = Command::new("clang")
        .args(&[
            "-fobjc-arc",
            "-framework", "Foundation",
            "-framework", "CoreGraphics",
            "-framework", "AppKit",
            "-o", dest_path.to_str().unwrap(),
            "native/awake_display.m",
        ])
        .status()
        .expect("Failed to compile awake_display.m");

    if !status.success() {
        panic!("Failed to compile awake_display.m");
    }

    println!("cargo:rerun-if-changed=native/awake_display.m");
    println!("cargo:rerun-if-changed=build.rs");

    tauri_build::build()
}
