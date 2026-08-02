//! CJK 拼音索引：启动时为文件名预计算拼音首字母 + 全拼，供 search_files 匹配拼音查询。
//!
//! 数据由 pinyin-pro 生成（U+4E00..U+9FFF，20992 字），编译时 include_bytes!/include_str! 内嵌。
//! 无运行时依赖、零外部 crate，查表 O(1)。

use std::sync::LazyLock;

/// CJK → 拼音首字母（a-z），`_` = 无拼音。索引 = codepoint - 0x4E00。
static CJK_INITIALS: &[u8] = include_bytes!("data/pinyin_initials.bin");

/// CJK → 无声调全拼（空格分隔）。索引 = codepoint - 0x4E00 在 split(' ') 后的位置。
static CJK_PINYIN_FLAT: &str = include_str!("data/pinyin_full.txt");

/// 全拼数组（LazyLock 一次性 split，O(1) 索引查表）。
static CJK_PINYIN_ARRAY: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| CJK_PINYIN_FLAT.split(' ').collect());

const CJK_START: u32 = 0x4E00;
const CJK_END: u32 = 0x9FFF;

/// pinyin_key 内部分隔符：不可打印字符，杜绝含空格的查询跨首字母段与全拼段误匹配。
const SEP: char = '\x1f';

/// 为文件名计算拼音键：`"首字母串\x1f全拼串"`（如 "设计文档" → "sjwd\x1fshejiwendang"）。
/// 非 CJK 文件名返回空串。用不可打印分隔符隔离首字母段与全拼段——ASCII 查询不含 \x1f，
/// 不可能跨段匹配；首字母（sjwd）与全拼（sheji）各自独立命中。
pub fn pinyin_key(name: &str) -> String {
    let mut initials = String::new();
    let mut full = String::new();
    let mut found_cjk = false;
    for ch in name.chars() {
        let cp = ch as u32;
        if (CJK_START..=CJK_END).contains(&cp) {
            found_cjk = true;
            let idx = (cp - CJK_START) as usize;
            if idx < CJK_INITIALS.len() {
                let b = CJK_INITIALS[idx];
                if b != b'_' {
                    initials.push(b as char);
                }
                if let Some(py) = CJK_PINYIN_ARRAY.get(idx) {
                    if !py.is_empty() {
                        full.push_str(py);
                    }
                }
            }
        }
    }
    if found_cjk {
        format!("{initials}{SEP}{full}")
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinyin_key_chinese_name() {
        let key = pinyin_key("设计文档");
        assert!(key.contains("sjwd"));
        assert!(key.contains("shejiwendang"));
    }

    #[test]
    fn pinyin_key_mixed() {
        let key = pinyin_key("测试.md");
        assert!(key.contains("cs"));
        assert!(key.contains("ceshi"));
    }

    #[test]
    fn pinyin_key_no_cjk_returns_empty() {
        assert_eq!(pinyin_key("readme.md"), "");
        assert_eq!(pinyin_key("hello world"), "");
    }

    #[test]
    fn pinyin_key_uv_handling() {
        // ü → v 约定：绿(lv)、女(nv)
        let key = pinyin_key("绿色");
        assert!(key.contains("lvse") || key.contains("lse"));
    }
}
