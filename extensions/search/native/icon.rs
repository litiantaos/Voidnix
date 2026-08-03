use objc2::rc::autoreleasepool;
use objc2::{runtime::AnyObject, AnyThread};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSImageInterpolation, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

/// 提取应用图标为 base64 PNG（64×64，实时提取，不缓存到磁盘）。
pub(super) fn get_app_icon(app_path: &str) -> Option<String> {
    // spawn_blocking 线程无 NSAutoreleasePool：所有 ObjC 图形操作（NSWorkspace/lockFocus/
    // NSBitmapImageRep/TIFFRepresentation）产生的 autorelease 对象须在 pool 内及时释放，
    // 否则长时间运行累积致内存耗尽、图标提取静默失败（实测 1000 次 ΔRSS 613MB→1.9MB）。
    let result = autoreleasepool(|_| extract_app_icon(app_path));
    match result {
        Some(base64_str) => {
            log::debug!("[icon] Extracted for: {}", app_path);
            Some(base64_str)
        }
        None => {
            log::warn!("[icon] All extraction methods failed for: {}", app_path);
            None
        }
    }
}

fn extract_app_icon(app_path: &str) -> Option<String> {
    if let Some(base64_str) = extract_icon_via_workspace(app_path, false) {
        return Some(base64_str);
    }

    log::info!(
        "[icon] NSWorkspace failed for {}, registering with LS and retrying",
        app_path
    );
    register_app_with_ls(app_path);
    if let Some(base64_str) = extract_icon_via_workspace(app_path, true) {
        return Some(base64_str);
    }

    log::info!(
        "[icon] NSWorkspace failed after LS registration for {}, trying bundle icns",
        app_path
    );
    extract_icon_from_bundle(app_path)
}

fn register_app_with_ls(app_path: &str) {
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if std::path::Path::new(lsregister).exists() {
        let child = std::process::Command::new(lsregister)
            .arg("-f")
            .arg(app_path)
            .spawn();
        if let Err(e) = child {
            log::warn!("[icon] lsregister spawn failed for {}: {}", app_path, e);
        }
    }
}

fn extract_icon_via_workspace(app_path: &str, after_ls_register: bool) -> Option<String> {
    if after_ls_register {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    // SAFETY: NSString/NSWorkspace/iconForFile: 均为标准选择子；
    // path_str 由 NSString::from_str 构造（合法 Retained<NSString>）；
    // 返回的 icon 为 Retained（ARC 托管），nsimage_to_png_base64 只读消费
    unsafe {
        let path_str = NSString::from_str(app_path);
        let workspace = NSWorkspace::sharedWorkspace();
        #[allow(deprecated)]
        let icon = workspace.iconForFile(&path_str);
        nsimage_to_png_base64(&icon, false)
    }
}

fn extract_icon_from_bundle(app_path: &str) -> Option<String> {
    let plist_path = std::path::Path::new(app_path)
        .join("Contents")
        .join("Info.plist");
    if !plist_path.exists() {
        return None;
    }

    let dict = plist::Value::from_file(&plist_path)
        .ok()
        .and_then(|v| v.into_dictionary())?;

    let icon_name = match dict.get("CFBundleIconFile").and_then(|v| v.as_string()) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => return None,
    };

    let icon_name = if icon_name.ends_with(".icns") {
        icon_name
    } else {
        format!("{}.icns", icon_name)
    };

    let resources_dir = std::path::Path::new(app_path)
        .join("Contents")
        .join("Resources");
    let icon_path = resources_dir.join(&icon_name);

    if !icon_path.exists() {
        return find_icns_in_resources(&resources_dir, &icon_name)
            .and_then(|p| convert_icns_to_png(&p, app_path));
    }

    convert_icns_to_png(&icon_path, app_path)
}

fn find_icns_in_resources(
    resources_dir: &std::path::Path,
    icon_name: &str,
) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(resources_dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_icns_in_resources(&path, icon_name) {
                return Some(found);
            }
        } else if path.file_name().and_then(|n| n.to_str()) == Some(icon_name) {
            return Some(path);
        }
    }
    None
}

fn convert_icns_to_png(icon_path: &std::path::Path, app_path: &str) -> Option<String> {
    let icon_path_str = icon_path.to_str()?;

    // SAFETY: path_ns 由 NSString::from_str 构造；initWithContentsOfFile: 为 NSImage 标准
    // 选择子，返回 Option（Some 时合法 Retained）；nsimage_to_png_base64 只读消费 image
    unsafe {
        let path_ns = NSString::from_str(icon_path_str);
        if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &path_ns) {
            if let Some(result) = nsimage_to_png_base64(&image, true) {
                log::info!("[icon] Extracted via NSImage with padding: {}", app_path);
                return Some(result);
            }
        }
    }

    log::info!("[icon] NSImage failed, trying sips for: {}", app_path);

    let tmp_path = std::env::temp_dir().join(format!(
        "voidnix-icon-{}.png",
        icon_path.file_stem()?.to_str()?
    ));
    // TempHandle 包裹：Drop 自动删文件，覆盖 sips 失败 / NSImage 处理异常 / 提前 return 等所有路径
    let _tmp_guard = crate::runtime::storage::TempHandle::new(tmp_path.clone());
    let output = std::process::Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("png")
        .arg(icon_path_str)
        .arg("--out")
        .arg(&tmp_path)
        .output()
        .ok()?;

    if output.status.success() {
        // SAFETY: tmp_ns 由 NSString::from_str 构造；initWithContentsOfFile: 返回 Option；
        // nsimage_to_png_base64 只读消费 image（Retained 托管）
        unsafe {
            let tmp_ns = NSString::from_str(tmp_path.to_str()?);
            if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &tmp_ns) {
                return nsimage_to_png_base64(&image, true);
            }
        }
    }

    None
}

unsafe fn nsimage_to_png_base64(image: &NSImage, add_padding: bool) -> Option<String> {
    let canvas_size = NSSize::new(64.0, 64.0);
    let new_image = NSImage::initWithSize(NSImage::alloc(), canvas_size);

    #[allow(deprecated)]
    new_image.lockFocus();

    if let Some(ctx) = NSGraphicsContext::currentContext() {
        ctx.setImageInterpolation(NSImageInterpolation(3));
    }

    let (dest_rect, src_rect) = if add_padding {
        let padding = 64.0 * 0.15;
        let icon_size = 64.0 - padding * 2.0;
        (
            NSRect::new(
                NSPoint::new(padding, padding),
                NSSize::new(icon_size, icon_size),
            ),
            NSRect::new(NSPoint::new(0.0, 0.0), image.size()),
        )
    } else {
        (
            NSRect::new(NSPoint::new(0.0, 0.0), canvas_size),
            NSRect::new(NSPoint::new(0.0, 0.0), image.size()),
        )
    };

    image.drawInRect_fromRect_operation_fraction(
        dest_rect,
        src_rect,
        NSCompositingOperation::Copy,
        1.0,
    );

    #[allow(deprecated)]
    new_image.unlockFocus();

    let tiff_data = new_image.TIFFRepresentation()?;
    let bitmap_rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff_data)?;

    let empty_dict = NSDictionary::<NSString, AnyObject>::new();
    let png_data =
        bitmap_rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_dict)?;

    let bytes = png_data.to_vec();
    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    ))
}
