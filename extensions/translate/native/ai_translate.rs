use crate::runtime::llm::{self, LlmMessage};

use super::TranslateResult;
use super::lang_utils::{
    build_system_prompt, detect_source_lang_name, lang_code_to_name, render_prompt,
    resolve_template, smart_target_lang, DEFAULT_TRANSLATE_PROMPT,
};

static PREAMBLE_PATTERNS: &[&str] = &[
    "here is the translation",
    "here's the translation",
    "the translation is",
    "translated text",
    "translation result",
    "translation:",
    "以下是翻译",
    "翻译结果",
    "翻译如下",
    "翻译：",
    "翻译:",
];

pub fn clean_translation(raw: &str) -> String {
    let mut s = raw.trim().to_string();

    if s.starts_with("```") && s.ends_with("```") {
        let inner = &s[3..s.len() - 3];
        let inner = inner.trim_start_matches(|c: char| c.is_alphanumeric() || c == '+');
        s = inner.trim().to_string();
    }

    let mut pair_strip = |open: char, close: char| {
        if s.starts_with(open) && s.ends_with(close) && s.len() > 2 {
            s = s[1..s.len() - 1].trim().to_string();
        }
    };
    pair_strip('"', '"');
    pair_strip('"', '"');
    pair_strip('「', '」');
    pair_strip('『', '』');

    let lower = s.to_lowercase();
    for pat in PREAMBLE_PATTERNS {
        if lower.starts_with(pat) {
            let remaining = &s[pat.len()..];
            let trimmed = remaining.trim_start_matches([':', '\u{ff1a}', ' ', '\n']);
            if !trimmed.is_empty() {
                s = trimmed.to_string();
            }
            break;
        }
    }

    s
}

struct PrepareResult {
    text: String,
    safe_endpoint: String,
    system_content: String,
    rendered: String,
}

fn prepare_ai_translate(
    text: &str,
    endpoint: &str,
    api_key: &str,
    model: &str,
    target_lang: Option<&str>,
    prompt: Option<&String>,
) -> Result<PrepareResult, String> {
    if text.trim().is_empty() {
        return Err("文本不能为空".to_string());
    }

    let safe_endpoint = llm::validate_ai_request(endpoint, model, api_key)?;

    let to_lang_code = smart_target_lang(text, target_lang.unwrap_or("zh"));
    let from_lang_name = detect_source_lang_name(text);
    let to_lang_name = lang_code_to_name(&to_lang_code);

    let template = resolve_template(prompt, DEFAULT_TRANSLATE_PROMPT);
    let rendered = render_prompt(template, text, from_lang_name, to_lang_name);
    let system_content = build_system_prompt(&to_lang_code);

    Ok(PrepareResult {
        text: text.to_string(),
        safe_endpoint,
        system_content,
        rendered,
    })
}

#[tauri::command]
pub async fn translate_ai(
    text: String,
    endpoint: String,
    api_key: String,
    model: String,
    target_lang: Option<String>,
    prompt: Option<String>,
) -> Result<TranslateResult, String> {
    let p = prepare_ai_translate(&text, &endpoint, &api_key, &model, target_lang.as_deref(), prompt.as_ref())?;

    // 复用 runtime/llm 共享非流式管道（validate + messages_to_json + map_api_error）
    let messages = vec![
        LlmMessage::system(p.system_content),
        LlmMessage::user(p.rendered),
    ];
    let raw = llm::openai_request_once(&endpoint, &api_key, &model, messages).await?;
    let translation = clean_translation(&raw);

    if translation.is_empty() {
        return Err("翻译返回为空".to_string());
    }

    let engine_label = llm::parse_scheme_host(&p.safe_endpoint)
        .map(|(_, host)| {
            let parts: Vec<&str> = host.split('.').collect();
            if parts.len() >= 2 {
                parts[parts.len() - 2].to_uppercase()
            } else {
                model.trim().to_string()
            }
        })
        .unwrap_or_else(|| model.trim().to_string());

    Ok(TranslateResult {
        source: p.text,
        translation,
        engine: engine_label,
    })
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn translate_ai_stream(
    app: tauri::AppHandle,
    text: String,
    endpoint: String,
    api_key: String,
    model: String,
    target_lang: Option<String>,
    prompt: Option<String>,
    request_id: String,
) -> Result<(), String> {
    let p = prepare_ai_translate(&text, &endpoint, &api_key, &model, target_lang.as_deref(), prompt.as_ref())?;

    let messages = vec![
        LlmMessage::system(p.system_content),
        LlmMessage::user(p.rendered),
    ];

    llm::stream_openai_request(llm::StreamConfig {
        app: &app,
        endpoint: &p.safe_endpoint,
        api_key: &api_key,
        model: &model,
        messages,
        tools: None,
        tool_choice: None,
        on_text_delta: None,
        on_tool_calls_delta: None,
        chunk_event: "translate-chunk",
        done_event: "translate-done",
        request_id: &request_id,
        abort_flag: None,
    })
    .await
    .map(|_| ())
}
