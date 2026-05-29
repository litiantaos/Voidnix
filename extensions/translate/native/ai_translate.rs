use crate::infra::sse::{self, ChatMessage};

use super::TranslateResult;
use super::lang_utils::{
    build_system_prompt, detect_source_lang_name, lang_code_to_name, render_prompt,
    resolve_template, smart_target_lang, DEFAULT_TRANSLATE_PROMPT,
};

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

    let (_scheme, safe_endpoint) = sse::validate_endpoint(endpoint)?;

    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }

    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }

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
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn translate_ai(
    text: String,
    endpoint: String,
    api_key: String,
    model: String,
    target_lang: Option<String>,
    prompt: Option<String>,
) -> Result<TranslateResult, String> {
    let p = prepare_ai_translate(&text, &endpoint, &api_key, &model, target_lang.as_deref(), prompt.as_ref())?;

    let url = format!(
        "{}/chat/completions",
        p.safe_endpoint.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "model": model.trim(),
        "messages": [
            {
                "role": "system",
                "content": p.system_content
            },
            {
                "role": "user",
                "content": p.rendered
            }
        ],
        "stream": false
    });

    let response = crate::infra::http::client()
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key.trim()))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("网络错误: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let _body_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 => "认证失败，请检查 API Key".to_string(),
            403 => "访问被拒绝，API Key 可能没有权限".to_string(),
            429 => "请求过于频繁，请稍后重试".to_string(),
            500.. => "AI 服务端错误，请稍后重试".to_string(),
            _ => format!("AI 翻译错误: HTTP {}", status),
        });
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("JSON 解析错误: {}", e))?;

    let translation = json
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    if translation.is_empty() {
        return Err("翻译返回为空".to_string());
    }

    let engine_label = sse::parse_scheme_host(&p.safe_endpoint)
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
        ChatMessage {
            role: "system".to_string(),
            content: p.system_content,
        },
        ChatMessage {
            role: "user".to_string(),
            content: p.rendered,
        },
    ];

    sse::stream_openai_request(sse::StreamConfig {
        app: &app,
        endpoint: &p.safe_endpoint,
        api_key: &api_key,
        model: &model,
        messages,
        chunk_event: "translate-chunk",
        done_event: "translate-done",
        request_id: &request_id,
    })
    .await
}
