pub fn is_chinese_dominant(text: &str) -> bool {
    let chinese_chars = text
        .chars()
        .filter(|c| {
            let cp = *c as u32;
            (0x4E00..=0x9FFF).contains(&cp)
                || (0x3400..=0x4DBF).contains(&cp)
                || (0xF900..=0xFAFF).contains(&cp)
        })
        .count();

    let total_significant = text
        .chars()
        .filter(|c| {
            c.is_alphabetic() || {
                let cp = *c as u32;
                (0x4E00..=0x9FFF).contains(&cp)
                    || (0x3400..=0x4DBF).contains(&cp)
                    || (0xF900..=0xFAFF).contains(&cp)
            }
        })
        .count();

    total_significant > 0 && (chinese_chars as f32 / total_significant as f32) > 0.3
}

pub fn detect_source_lang_name(text: &str) -> &'static str {
    if is_chinese_dominant(text) {
        "中文"
    } else {
        "英文"
    }
}

pub fn smart_target_lang(text: &str, target_lang: &str) -> String {
    let is_chinese = is_chinese_dominant(text);
    match target_lang {
        "zh" => {
            if is_chinese {
                "en".to_string()
            } else {
                "zh".to_string()
            }
        }
        "en" => {
            if is_chinese {
                "en".to_string()
            } else {
                "zh".to_string()
            }
        }
        other => {
            if is_chinese {
                other.to_string()
            } else {
                "zh".to_string()
            }
        }
    }
}

pub fn lang_code_to_name(code: &str) -> &str {
    match code {
        "zh" => "中文",
        "en" => "英文",
        "ja" => "日文",
        "ko" => "韩文",
        "fr" => "法文",
        "de" => "德文",
        "es" => "西班牙文",
        _ => code,
    }
}

pub fn lang_code_to_name_en(code: &str) -> &str {
    match code {
        "zh" => "Chinese",
        "en" => "English",
        "ja" => "Japanese",
        "ko" => "Korean",
        "fr" => "French",
        "de" => "German",
        "es" => "Spanish",
        _ => code,
    }
}

pub fn build_system_prompt(to_lang: &str) -> String {
    format!(
        "You are a professional translator. Translate the user text to {}.\n\
         Rules:\n\
         - Output ONLY the translated text.\n\
         - No explanations, notes, commentary, or preamble.\n\
         - No quotes, no markdown, no code blocks.\n\
         - Preserve the original tone, formatting, and meaning.",
        lang_code_to_name_en(to_lang)
    )
}

pub const DEFAULT_TRANSLATE_PROMPT: &str = "{text}";

pub fn render_prompt(template: &str, text: &str, from_lang: &str, to_lang: &str) -> String {
    template
        .replace("{text}", text)
        .replace("{fromLang}", from_lang)
        .replace("{toLang}", to_lang)
}

pub fn resolve_template<'a>(prompt: Option<&'a String>, fallback: &'a str) -> &'a str {
    match prompt {
        Some(t) if !t.trim().is_empty() => t,
        _ => fallback,
    }
}
