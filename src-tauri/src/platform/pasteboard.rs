//! NSPasteboard 统一接口。
//!
//! 所有 NSPasteboard 操作的唯一入口。clipboard 扩展的 monitor/commands 均委托至此。

#![allow(dead_code)]

use objc2_app_kit::{
    NSPasteboard, NSPasteboardTypeFileURL, NSPasteboardTypePNG, NSPasteboardTypeString,
};
use objc2_foundation::{NSData, NSString};

/// 读取剪贴板文本。
pub fn read_text() -> Option<String> {
    unsafe {
        NSPasteboard::generalPasteboard()
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string())
    }
}

/// 读取文件 URL（NSPasteboardTypeFileURL）。
pub fn read_file_url() -> Option<String> {
    unsafe {
        NSPasteboard::generalPasteboard()
            .stringForType(NSPasteboardTypeFileURL)
            .map(|s| s.to_string())
    }
}

/// 读取 PNG 原始字节（NSPasteboardTypePNG）。
pub fn read_png() -> Option<Vec<u8>> {
    unsafe {
        NSPasteboard::generalPasteboard()
            .dataForType(NSPasteboardTypePNG)
            .map(|d| d.to_vec())
    }
}

/// 清空剪贴板。
pub fn clear() {
    NSPasteboard::generalPasteboard().clearContents();
}

/// 写入纯文本（清空 + 写 public.utf8-plain-text）。供前端 pasteboard_write_text 与 finder-ext 消费。
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

/// 写 NSPasteboardTypePNG（不清空）。
pub fn set_png(bytes: &[u8]) {
    let d = NSData::with_bytes(bytes);
    NSPasteboard::generalPasteboard().setData_forType(Some(&d), unsafe { NSPasteboardTypePNG });
}

/// 写自定义 UTI 类型字符串（不清空，供 clipboard marker 等自定义类型）。
pub fn set_custom(s: &str, type_uti: &str) {
    let ns = NSString::from_str(s);
    let ty = NSString::from_str(type_uti);
    NSPasteboard::generalPasteboard().setString_forType(&ns, &ty);
}

/// 框架命令：前端 invoke('pasteboard_write_text')。替代 tauri-plugin-clipboard-manager。
#[tauri::command]
pub fn pasteboard_write_text(text: String) {
    write_text(&text);
}

/// 按类型名读取字符串值。
pub fn string_for_type(type_name: &str) -> Option<String> {
    let ns_type = NSString::from_str(type_name);
    NSPasteboard::generalPasteboard()
        .stringForType(&ns_type)
        .map(|s| s.to_string())
}

/// 按类型名读取原始数据。
pub fn data_for_type(type_name: &str) -> Option<Vec<u8>> {
    let ns_type = NSString::from_str(type_name);
    NSPasteboard::generalPasteboard()
        .dataForType(&ns_type)
        .map(|d| d.to_vec())
}

/// 检查剪贴板是否包含指定类型。
pub fn has_type(type_name: &str) -> bool {
    let ns_type = NSString::from_str(type_name);
    NSPasteboard::generalPasteboard()
        .types()
        .map(|types| types.containsObject(&ns_type))
        .unwrap_or(false)
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
    PasteboardSnapshot { change_count, items }
}

/// 恢复剪贴板到快照状态。
pub fn restore(snap: &PasteboardSnapshot) {
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{NSPasteboardItem, NSPasteboardWriting};
    use objc2_foundation::{NSArray, NSData, NSString};
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    if snap.items.is_empty() {
        return;
    }
    let mut protos: Vec<objc2::rc::Retained<ProtocolObject<dyn NSPasteboardWriting>>> = Vec::new();
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
}
