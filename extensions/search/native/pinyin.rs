use once_cell::sync::Lazy;
use pinyin::ToPinyin;

pub(super) fn to_pinyin_full(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c.to_ascii_lowercase().to_string())
            } else {
                c.to_pinyin()
                    .map(|p| p.plain().to_string())
                    .or_else(|| Some(c.to_string()))
            }
        })
        .collect()
}

static PINYIN_WORDS: Lazy<Vec<(&str, &str)>> = Lazy::new(|| {
    vec![
        ("音乐", "yinyue"),
        ("相册", "xiangce"),
        ("长", "chang"),
        ("行", "hang"),
        ("重命", "chongming"),
        ("地图", "ditu"),
    ]
});

pub(super) fn word_pinyin_overrides(name: &str) -> String {
    PINYIN_WORDS
        .iter()
        .filter_map(|(word, pinyin)| {
            if name.contains(word) {
                Some(*pinyin)
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn to_pinyin_initials(name: &str) -> String {
    name.chars()
        .filter_map(|c| {
            if c.is_ascii() {
                Some(c.to_ascii_lowercase())
            } else {
                c.to_pinyin()
                    .map(|p| p.plain().chars().next().unwrap_or(c))
                    .or(Some(c))
            }
        })
        .collect()
}
