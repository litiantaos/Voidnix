//! NSPasteboard 统一接口。
//!
//! 所有 NSPasteboard 操作的唯一入口；monitor/commands 等上层均委托至此。
//!
//! H6：所有返回 autoreleased NSString/NSData 的读路径包 autoreleasepool，
//! 避免在后台线程（translate 划词、clipboard monitor 等）累积内存。

use objc2::rc::autoreleasepool;
use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
    NSPasteboardTypeTIFF,
};
use objc2_foundation::{NSData, NSString, NSURL};

/// 读取剪贴板文本。
pub fn read_text() -> Option<String> {
    autoreleasepool(|_| unsafe {
        NSPasteboard::generalPasteboard()
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string())
    })
}

/// 读取所有文件 URL（多选复制时剪贴板含多个 NSPasteboardItem，每个一个 file URL）。
/// 单文件复制返回 1 元素 Vec。
pub fn read_file_urls() -> Vec<String> {
    autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let mut urls = Vec::new();
        if let Some(items) = pb.pasteboardItems() {
            for item in items.iter() {
                if let Some(s) = item.stringForType(NSPasteboardTypeFileURL) {
                    urls.push(s.to_string());
                }
            }
        }
        urls
    })
}

/// 把 file URL（可能是 file reference URL `file:///.file/id=...`）解析为实际文件路径。
/// Finder 复制文件写入 file reference URL（基于文件 id），无法直接 fs::read，
/// 需经 NSURL.filePathURL 转为 path-based URL 再取 path。
pub fn resolve_file_url_to_path(url: &str) -> Option<String> {
    autoreleasepool(|_| {
        let nsurl = NSURL::URLWithString(&NSString::from_str(url))?;
        let path_url = nsurl.filePathURL()?;
        let path = path_url.path()?;
        Some(path.to_string())
    })
}

/// 读取 PNG 原始字节（NSPasteboardTypePNG）。
/// `max_bytes`：源 NSData 超限则丢弃（先量后拷，避免大图 `to_vec` 与后续 base64）。
pub fn read_png(max_bytes: u64) -> Option<Vec<u8>> {
    autoreleasepool(|_| {
        let d = unsafe { NSPasteboard::generalPasteboard().dataForType(NSPasteboardTypePNG) }?;
        let len = d.len() as u64;
        if len == 0 || len > max_bytes {
            return None;
        }
        Some(d.to_vec())
    })
}

/// 读取 TIFF 数据并转为 PNG 字节（微信、预览程序等写 NSPasteboardTypeTIFF）。
/// 用 NSBitmapImageRep 解码 TIFF 后重新编码为 PNG，供 base64 data URL 复用。
///
/// `max_bytes` 同时约束：
/// 1. **源 TIFF** 超限 → 不解码、直接丢弃（有意收紧：不赌转码后 PNG 更小，避免主线程尖峰）
/// 2. 转码后 PNG 超限 → 丢弃（与入库/base64 上限对齐）
pub fn read_tiff_as_png(max_bytes: u64) -> Option<Vec<u8>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep};
    use objc2_foundation::NSDictionary;
    autoreleasepool(|_| unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let tiff_data = pb.dataForType(NSPasteboardTypeTIFF)?;
        let src_len = tiff_data.len() as u64;
        if src_len == 0 || src_len > max_bytes {
            return None;
        }
        let rep = NSBitmapImageRep::imageRepWithData(&tiff_data)?;
        let empty = NSDictionary::<objc2_foundation::NSString, objc2::runtime::AnyObject>::new();
        let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty)?;
        let out = png.to_vec();
        // 转码膨胀后仍超限则丢弃（入库/base64 与磁盘图上限对齐）
        if out.is_empty() || (out.len() as u64) > max_bytes {
            return None;
        }
        Some(out)
    })
}

/// 清空剪贴板。
pub fn clear() {
    NSPasteboard::generalPasteboard().clearContents();
}

/// 写入纯文本（清空 + 写 public.utf8-plain-text）。上层命令薄壳在 runtime::pasteboard。
pub fn write_text(s: &str) {
    clear();
    set_string(s);
}

/// 写 NSPasteboardTypeString（不清空）。
pub fn set_string(s: &str) {
    unsafe {
        let ns = NSString::from_str(s);
        NSPasteboard::generalPasteboard().setString_forType(&ns, NSPasteboardTypeString);
    }
}

/// 写 NSPasteboardTypeFileURL（不清空）。
pub fn set_file_url(s: &str) {
    unsafe {
        let ns = NSString::from_str(s);
        NSPasteboard::generalPasteboard().setString_forType(&ns, NSPasteboardTypeFileURL);
    }
}

/// 写多个 file URL：每个 NSPasteboardItem 一个（对齐 Finder 多选复制）。
/// 单元素退化为 `set_file_url`（+ 可选 pasteboard 级 marker）；空切片 no-op。
/// 调用前通常先 `clear()`。
///
/// `marker_uti`：挂到每个 item 的防回环 UTI。`writeObjects` 会替换 pasteboard items，
/// 先 `set_custom` 再写多 URL 会丢失 marker，故多文件必须把 UTI 写进 item 本身。
pub fn set_file_urls(urls: &[String], marker_uti: Option<&str>) {
    if urls.is_empty() {
        return;
    }
    if urls.len() == 1 {
        set_file_url(&urls[0]);
        if let Some(m) = marker_uti {
            set_custom("", m);
        }
        return;
    }
    autoreleasepool(|_| {
        use objc2::runtime::ProtocolObject;
        use objc2_app_kit::{NSPasteboardItem, NSPasteboardWriting};
        use objc2_foundation::NSArray;
        let pb = NSPasteboard::generalPasteboard();
        let mut protos: Vec<objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>> =
            Vec::with_capacity(urls.len());
        for url in urls {
            let item = NSPasteboardItem::new();
            let ns = NSString::from_str(url);
            // setString_forType 对 file URL 项足够；与 read_file_urls 对称
            item.setString_forType(&ns, unsafe { NSPasteboardTypeFileURL });
            if let Some(m) = marker_uti {
                let marker = NSString::from_str("");
                let ty = NSString::from_str(m);
                item.setString_forType(&marker, &ty);
            }
            protos.push(ProtocolObject::from_retained(item));
        }
        let array = NSArray::from_retained_slice(&protos);
        pb.writeObjects(&array);
    });
}

/// 任意图片字节 → PNG（NSImage 解码；不触碰 pasteboard）。
/// NSImage 支持 PNG/JPEG/GIF/WebP/BMP/HEIC 等；NSBitmapImageRep 不支持 HEIC，故经 NSImage → TIFF 中转。
pub fn encode_image_to_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use objc2::AnyThread;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::NSDictionary;
    autoreleasepool(|_| unsafe {
        let data = NSData::with_bytes(bytes);
        let image = NSImage::initWithData(NSImage::alloc(), &data)?;
        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let empty = NSDictionary::<NSString, objc2::runtime::AnyObject>::new();
        let png = rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &empty)?;
        let out = png.to_vec();
        if out.is_empty() {
            return None;
        }
        Some(out)
    })
}

/// 写已是 PNG 的字节到 `NSPasteboardTypePNG`（不清空）。
/// 与 `encode_image_to_png` 配对：先 encode 校验，再 clear + 本函数，避免失败时剪贴板已空。
pub fn set_png_bytes(png: &[u8]) {
    unsafe {
        let data = NSData::with_bytes(png);
        NSPasteboard::generalPasteboard().setData_forType(Some(&data), NSPasteboardTypePNG);
    }
}

/// 写自定义 UTI 类型字符串（不清空，供写入自定义标记类型用于自识别）。
pub fn set_custom(s: &str, type_uti: &str) {
    let ns = NSString::from_str(s);
    let ty = NSString::from_str(type_uti);
    NSPasteboard::generalPasteboard().setString_forType(&ns, &ty);
}

/// 检查剪贴板是否包含指定类型。
pub fn has_type(type_name: &str) -> bool {
    autoreleasepool(|_| {
        let ns_type = NSString::from_str(type_name);
        NSPasteboard::generalPasteboard()
            .types()
            .map(|types| types.containsObject(&ns_type))
            .unwrap_or(false)
    })
}

/// 获取剪贴板 changeCount（用于轮询检测变化）。
pub fn change_count() -> isize {
    NSPasteboard::generalPasteboard().changeCount()
}

/// 剪贴板快照（用于操作后恢复）。
pub struct PasteboardSnapshot {
    pub change_count: isize,
    items: Vec<Vec<(String, Vec<u8>)>>,
}

/// 保存当前剪贴板完整状态。
pub fn snapshot() -> PasteboardSnapshot {
    autoreleasepool(|_| {
        let pb = NSPasteboard::generalPasteboard();
        let change_count = pb.changeCount();
        let mut items = Vec::new();
        if let Some(ns_items) = pb.pasteboardItems() {
            for item in ns_items.iter() {
                let mut entries = Vec::new();
                for t in item.types().iter() {
                    if let Some(data) = item.dataForType(&t) {
                        entries.push((t.to_string(), data.to_vec()));
                    }
                }
                items.push(entries);
            }
        }
        PasteboardSnapshot {
            change_count,
            items,
        }
    })
}

/// 恢复剪贴板到快照状态。
pub fn restore(snap: &PasteboardSnapshot) {
    autoreleasepool(|_| {
        use objc2::runtime::ProtocolObject;
        use objc2_app_kit::{NSPasteboardItem, NSPasteboardWriting};
        use objc2_foundation::{NSArray, NSData, NSString};
        let pb = NSPasteboard::generalPasteboard();
        pb.clearContents();
        if snap.items.is_empty() {
            return;
        }
        let mut protos: Vec<objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>> =
            Vec::new();
        for entries in &snap.items {
            let item = NSPasteboardItem::new();
            for (t, bytes) in entries {
                let ns_type = NSString::from_str(t);
                let ns_data = NSData::with_bytes(bytes);
                item.setData_forType(&ns_data, &ns_type);
            }
            protos.push(ProtocolObject::from_retained(item));
        }
        let array = NSArray::from_retained_slice(&protos);
        pb.writeObjects(&array);
    });
}
