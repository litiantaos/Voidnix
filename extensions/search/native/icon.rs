use std::fs;
use std::path::Path;

use objc2::{runtime::AnyObject, AnyThread};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSCompositingOperation, NSGraphicsContext, NSImage,
    NSImageInterpolation, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

fn get_bundle_mtime(app_path: &str) -> u64 {
    fs::metadata(app_path)
        .and_then(|m| m.modified())
        .map(|t| {
            t.duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
        .unwrap_or(0)
}

fn icon_cache_path(cache_dir: &Path, app_path: &str) -> std::path::PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mtime = get_bundle_mtime(app_path);
    let mut hasher = DefaultHasher::new();
    app_path.hash(&mut hasher);
    mtime.hash(&mut hasher);
    let hash = hasher.finish();
    cache_dir.join(format!("{}.png", hash))
}

pub(super) fn get_app_icon(app_path: &str) -> Option<String> {
    let cache_dir = crate::infra::path::icon_cache_dir();

    let cached_path = icon_cache_path(&cache_dir, app_path);

    if cached_path.exists() {
        if let Ok(bytes) = fs::read(&cached_path) {
            if !bytes.is_empty() {
                log::debug!("[icon] Cache hit for: {}", app_path);
                return Some(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    &bytes,
                ));
            }
        }
        let _ = fs::remove_file(&cached_path);
    }

    match extract_app_icon(app_path) {
        Some(base64_str) => {
            log::debug!("Icon extracted for: {}", app_path);
            Some(base64_str)
        }
        None => {
            log::warn!("Failed to extract icon for: {}", app_path);
            None
        }
    }
}

fn extract_app_icon(app_path: &str) -> Option<String> {
    let cache_dir = crate::infra::path::icon_cache_dir();

    let cached_path = icon_cache_path(&cache_dir, app_path);

    if let Some(base64_str) = extract_icon_via_workspace(app_path, &cached_path, false) {
        return Some(base64_str);
    }

    log::info!(
        "[icon] NSWorkspace failed for {}, registering with LS and retrying",
        app_path
    );
    register_app_with_ls(app_path);
    if let Some(base64_str) = extract_icon_via_workspace(app_path, &cached_path, true) {
        return Some(base64_str);
    }

    log::info!(
        "[icon] NSWorkspace failed after LS registration for {}, trying bundle icns",
        app_path
    );
    if let Some(base64_str) = extract_icon_from_bundle(app_path, &cached_path) {
        return Some(base64_str);
    }

    log::error!(
        "[icon] All extraction methods failed for: {}",
        app_path
    );
    None
}

fn register_app_with_ls(app_path: &str) {
    let lsregister = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister";
    if Path::new(lsregister).exists() {
        let child = std::process::Command::new(lsregister)
            .arg("-f")
            .arg(app_path)
            .spawn();
        match child {
            Ok(mut c) => match c.wait() {
                Ok(s) if s.success() => {
                    log::debug!("lsregister succeeded for: {}", app_path);
                }
                Ok(s) => {
                    log::warn!("lsregister exited with {} for: {}", s, app_path);
                }
                Err(e) => {
                    log::warn!("lsregister wait failed for {}: {}", app_path, e);
                }
            },
            Err(e) => {
                log::warn!("lsregister spawn failed: {}", e);
            }
        }
    }
}

fn extract_icon_via_workspace(
    app_path: &str,
    cached_path: &Path,
    after_ls_register: bool,
) -> Option<String> {
    if after_ls_register {
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    let result = unsafe {
        let path_str = NSString::from_str(app_path);
        let workspace = NSWorkspace::sharedWorkspace();

        #[allow(deprecated)]
        let icon = workspace.iconForFile(&path_str);
        nsimage_to_png_base64(&icon, cached_path)
    };

    if result.is_some() {
        log::debug!("[icon] NSWorkspace extracted for: {}", app_path);
    }
    result
}

fn extract_icon_from_bundle(app_path: &str, cached_path: &Path) -> Option<String> {
    let plist_path = Path::new(app_path)
        .join("Contents")
        .join("Info.plist");
    if !plist_path.exists() {
        log::warn!("[icon] Info.plist not found: {:?}", plist_path);
        return None;
    }

    let dict = plist::Value::from_file(&plist_path)
        .ok()
        .and_then(|v| v.into_dictionary())?;

    let icon_name = match dict
        .get("CFBundleIconFile")
        .and_then(|v| v.as_string())
    {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => {
            if let Some(asset_name) = dict.get("CFBundleIconName").and_then(|v| v.as_string()) {
                log::info!(
                    "[icon] {} uses Asset Catalog icon: {}",
                    app_path,
                    asset_name
                );
            } else {
                log::warn!(
                    "[icon] No CFBundleIconFile/CFBundleIconName in {}",
                    app_path
                );
            }
            return None;
        }
    };

    let icon_name = if icon_name.ends_with(".icns") {
        icon_name
    } else {
        format!("{}.icns", icon_name)
    };

    let resources_dir = Path::new(app_path)
        .join("Contents")
        .join("Resources");
    let icon_path = resources_dir.join(&icon_name);

    if !icon_path.exists() {
        let found = find_icns_in_resources(&resources_dir, &icon_name);
        match found {
            Some(path) => return convert_icns_to_png(&path, cached_path, app_path),
            None => {
                let icns_files: Vec<String> = fs::read_dir(&resources_dir)
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().is_some_and(|ext| ext == "icns"))
                    .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                    .collect();
                log::warn!(
                    "[icon] Icon not found: {:?}, available: {:?}",
                    icon_path,
                    icns_files
                );
                return None;
            }
        }
    }

    convert_icns_to_png(&icon_path, cached_path, app_path)
}

fn find_icns_in_resources(resources_dir: &Path, icon_name: &str) -> Option<std::path::PathBuf> {
    for entry in fs::read_dir(resources_dir).ok()?.flatten() {
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

fn convert_icns_to_png(icon_path: &Path, cached_path: &Path, app_path: &str) -> Option<String> {
    let icon_path_str = icon_path.to_str()?;

    unsafe {
        let path_ns = NSString::from_str(icon_path_str);
        if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &path_ns) {
            if let Some(result) =
                nsimage_to_png_base64_with_padding(&image, cached_path, true)
            {
                log::info!(
                    "[icon] Extracted via NSImage with padding: {}",
                    app_path
                );
                return Some(result);
            }
        }
    }

    log::info!(
        "[icon] NSImage failed, trying sips for: {}",
        app_path
    );

    let tmp_path = cached_path.with_extension("tmp.png");
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
        unsafe {
            let tmp_ns = NSString::from_str(tmp_path.to_str()?);
            if let Some(image) = NSImage::initWithContentsOfFile(NSImage::alloc(), &tmp_ns) {
                let result =
                    nsimage_to_png_base64_with_padding(&image, cached_path, true);
                let _ = fs::remove_file(&tmp_path);
                if result.is_some() {
                    log::info!(
                        "[icon] Extracted via sips with padding: {}",
                        app_path
                    );
                }
                return result;
            }
        }
        let _ = fs::remove_file(&tmp_path);
    } else {
        log::warn!(
            "[icon] sips failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    None
}

unsafe fn nsimage_to_png_base64(image: &NSImage, cached_path: &Path) -> Option<String> {
    nsimage_to_png_base64_with_padding(image, cached_path, false)
}

unsafe fn nsimage_to_png_base64_with_padding(
    image: &NSImage,
    cached_path: &Path,
    add_padding: bool,
) -> Option<String> {
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
            NSRect::new(NSPoint::new(padding, padding), NSSize::new(icon_size, icon_size)),
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
    let bitmap_rep =
        NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff_data)?;

    let empty_dict = NSDictionary::<NSString, AnyObject>::new();
    let png_data = bitmap_rep
        .representationUsingType_properties(NSBitmapImageFileType::PNG, &empty_dict)?;

    let bytes = png_data.to_vec();
    let _ = fs::write(cached_path, &bytes);

    Some(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &bytes,
    ))
}
