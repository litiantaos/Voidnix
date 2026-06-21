use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR not set");

    // 编译 awake_display.mm → 可执行文件
    let awake_dest = Path::new(&out_dir).join("awake_display");
    let status = Command::new("clang++")
        .args([
            "-fobjc-arc",
            "-std=c++17",
            "-mmacosx-version-min=11.0",
            "-framework",
            "Foundation",
            "-framework",
            "CoreGraphics",
            "-framework",
            "AppKit",
            "-o",
            awake_dest.to_str().unwrap(),
            "../extensions/awake/native/awake_display.mm",
        ])
        .status()
        .expect("Failed to compile awake_display.mm");

    if !status.success() {
        panic!("Failed to compile awake_display.mm");
    }

    println!("cargo:rerun-if-changed=../extensions/awake/native/awake_display.mm");
    println!("cargo:rerun-if-changed=build.rs");
    // cargo 1.46+ 自动追踪 #[path] 引用的文件（extensions.rs 中已声明），
    // 无需 rerun-if-changed=../extensions——那会导致修改任意扩展文件都重跑 build.rs，
    // 连带 zsh-autosuggestions binary target 被重链接覆盖。
    // 仅补充 cargo 不自动追踪的 include_bytes!/include_str! 引用：
    println!("cargo:rerun-if-changed=../public/bar-icon-fill.png");

    // tauri_build::build() 在编译期校验 tauri.conf.json 的 bundle.resources 文件存在。
    // debug 编译（cargo test/check/tauri dev）不走 beforeBuildCommand，release binary 不存在，
    // 此处创建空占位让校验通过；release 链路由 beforeBuildCommand 编译真实 binary 覆盖。
    // is_non_empty_binary（mod.rs）确保空占位不会被 install_bin 误部署。
    let zsh_bin = Path::new("target/release/zsh-autosuggestions");
    if !zsh_bin.exists() {
        let _ = std::fs::create_dir_all("target/release");
        let _ = std::fs::write(zsh_bin, "");
    }

    // 仅在 macOS 目标上编译原生桥接静态库
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        build_screenshot_overlay(&out_dir);
        link_skylight();
    }

    tauri_build::build()
}

/// 编译 native/screenshot_overlay.mm 为 libscreenshot_overlay.a。
/// 提供 CALayer 直贴 CGImage 的工业级背景层桥（替代 PNG/JPEG 编码 + WebView img 加载）。
fn build_screenshot_overlay(out_dir: &str) {
    let mm_src = "../extensions/screenshot/native/screenshot_overlay.mm";
    let mm_obj = Path::new(out_dir).join("screenshot_overlay.o");
    let lib_path = Path::new(out_dir).join("libscreenshot_overlay.a");

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
        .expect("failed to invoke clang++ for screenshot_overlay.mm");
    assert!(compile.success(), "clang++ failed to compile {mm_src}");

    let _ = std::fs::remove_file(&lib_path);
    let ar = Command::new("ar")
        .args(["rcs", lib_path.to_str().unwrap(), mm_obj.to_str().unwrap()])
        .status()
        .expect("failed to invoke ar for libscreenshot_overlay.a");
    assert!(ar.success(), "ar failed to archive libscreenshot_overlay.a");

    println!("cargo:rustc-link-search=native={out_dir}");
    println!("cargo:rustc-link-lib=static=screenshot_overlay");
    println!("cargo:rustc-link-lib=framework=QuartzCore");
    println!("cargo:rustc-link-lib=framework=ImageIO");
    println!("cargo:rerun-if-changed=../extensions/screenshot/native/screenshot_overlay.mm");
}

/// 链接 macOS 私有 framework SkyLight（用于 Space 迁移私有 API）。
/// SkyLight 不在标准 framework 搜索路径，需显式指定 /System/Library/PrivateFrameworks。
/// `tauri.conf.json` 已声明 `macOSPrivateApi: true`，项目本就走私有 API 路径。
fn link_skylight() {
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");
    println!("cargo:rustc-link-lib=framework=SkyLight");
}
