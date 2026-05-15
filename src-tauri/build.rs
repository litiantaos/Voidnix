use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");

    // 编译 awake_display.m → 可执行文件（不在本特性范围内，保持原状）
    let awake_dest = Path::new(&out_dir).join("awake_display");
    let status = Command::new("clang")
        .args([
            "-fobjc-arc",
            "-framework",
            "Foundation",
            "-framework",
            "CoreGraphics",
            "-framework",
            "AppKit",
            "-o",
            awake_dest.to_str().unwrap(),
            "native/awake_display.m",
        ])
        .status()
        .expect("Failed to compile awake_display.m");

    if !status.success() {
        panic!("Failed to compile awake_display.m");
    }

    println!("cargo:rerun-if-changed=native/awake_display.m");
    println!("cargo:rerun-if-changed=build.rs");

    // 仅在 macOS 目标上编译 webkit_tuning.mm 桥接静态库
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        build_webkit_tuning(&out_dir);
    }

    tauri_build::build()
}

/// 把 native/webkit_tuning.mm 编译为 libwebkit_tuning.a 并写入链接指令。
/// 流程：clang++ → .o，再 ar rcs → .a，输出到 OUT_DIR。
fn build_webkit_tuning(out_dir: &str) {
    let mm_src = "native/webkit_tuning.mm";
    let mm_obj = Path::new(out_dir).join("webkit_tuning.o");
    let lib_path = Path::new(out_dir).join("libwebkit_tuning.a");

    let compile = Command::new("clang++")
        .args([
            "-c",
            "-fobjc-arc",
            "-fmodules",
            "-std=c++17",
            "-mmacosx-version-min=11.0",
            "-o",
            mm_obj.to_str().unwrap(),
            mm_src,
        ])
        .status()
        .expect("failed to invoke clang++ for webkit_tuning.mm");
    assert!(compile.success(), "clang++ failed to compile {mm_src}");

    // 删旧的 .a，避免 ar rcs 在已存在的归档上反复追加
    let _ = std::fs::remove_file(&lib_path);

    let ar = Command::new("ar")
        .args([
            "rcs",
            lib_path.to_str().unwrap(),
            mm_obj.to_str().unwrap(),
        ])
        .status()
        .expect("failed to invoke ar for libwebkit_tuning.a");
    assert!(ar.success(), "ar failed to archive libwebkit_tuning.a");

    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=webkit_tuning");
    println!("cargo:rustc-link-lib=framework=WebKit");
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=dylib=c++");
    println!("cargo:rerun-if-changed=native/webkit_tuning.mm");
}
