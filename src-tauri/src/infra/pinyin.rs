use std::sync::LazyLock;
use pinyin::ToPinyin;
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Matcher, Utf32Str,
};
use std::cell::RefCell;

pub fn to_pinyin_full(name: &str) -> String {
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

static PINYIN_WORDS: LazyLock<Vec<(&str, &str)>> = LazyLock::new(|| {
    vec![
        ("音乐", "yinyue"),
        ("相册", "xiangce"),
        ("长", "chang"),
        ("行", "hang"),
        ("重命", "chongming"),
        ("地图", "ditu"),
    ]
});

pub fn word_pinyin_overrides(name: &str) -> String {
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

pub fn to_pinyin_initials(name: &str) -> String {
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

thread_local! {
    pub static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::default());
}

pub fn expand_pinyin(text: &str) -> String {
    let pinyin_full = to_pinyin_full(text);
    let pinyin_initials = to_pinyin_initials(text);
    let compact: String = pinyin_full.chars().filter(|c| !c.is_whitespace()).collect();
    let overrides = word_pinyin_overrides(text);
    format!(
        "{} {} {} {} {}",
        text, pinyin_full, pinyin_initials, compact, overrides
    )
}

pub fn pinyin_score(
    text: &str,
    pattern: &Pattern,
    matcher: &mut Matcher,
    buf: &mut Vec<char>,
) -> u32 {
    let expanded = expand_pinyin(text);
    pattern
        .score(Utf32Str::new(&expanded, buf), matcher)
        .unwrap_or(0)
}

pub fn match_query(query: &str) -> Pattern {
    Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart)
}
