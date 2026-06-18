//! NSPasteboard 统一接口。
//!
//! 替代原 text_selection + clipboard 扩展各自实现的 NSPasteboard 操作。
//! Phase 2 将让 clipboard 扩展的 monitor/commands 委托至此。

#![allow(dead_code)]

use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};

/// 读取剪贴板文本。
pub fn read_text() -> Option<String> {
    unsafe {
        NSPasteboard::generalPasteboard()
            .stringForType(NSPasteboardTypeString)
            .map(|s| s.to_string())
    }
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
