use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::time::Duration;

use crate::infra::sse::{self, ChatMessage};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct TranslateResult {
    pub source: String,
    pub translation: String,
    pub engine: String,
}

// ============================================================================
// Selected Text Extraction
// ============================================================================

/// Read clipboard with polling fallback.
///
/// The translate shortcut handler (in shortcut.rs) already executed AppleScript
/// Cmd+C before the window activated. This function reads the clipboard.
/// If the first read is empty (clipboard not yet updated), it polls for up to
/// 300ms before giving up.
#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn get_selected_text() -> Result<String, String> {
    use std::time::Instant;
    use tokio::process::Command;

    // First read
    let text = Command::new("pbpaste")
        .output()
        .await
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    if !text.trim().is_empty() {
        return Ok(text.trim().to_string());
    }

    // Clipboard empty — poll for up to 300ms (handles high system load)
    let start = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(20)).await;

        let text = Command::new("pbpaste")
            .output()
            .await
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        if !text.trim().is_empty() {
            return Ok(text.trim().to_string());
        }

        if start.elapsed() > Duration::from_millis(300) {
            break;
        }
    }

    Ok(String::new())
}

// ============================================================================
// Youdao Translation
// ============================================================================

#[tauri::command]
#[cfg_attr(feature = "specta", specta::specta)]
pub async fn translate_youdao(
    text: String,
    app_key: String,
    app_secret: String,
    target_lang: Option<String>,
) -> Result<TranslateResult, String> {
    if text.trim().is_empty() {
        return Err("文本不能为空".to_string());
    }

    // Youdao API limits: truncate to 5000 chars to avoid error 205/2000
    let text = match text.char_indices().nth(5000) {
        Some((byte_idx, _)) => text[..byte_idx].to_string(),
        None => text,
    };

    let resolved_lang = smart_target_lang(&text, target_lang.as_deref().unwrap_or("zh"));
    let salt = nonce();
    let curtime = chrono_timestamp();
    let input = truncate_for_sign(&text);

    let sign_input = format!("{}{}{}{}{}", app_key, input, salt, curtime, app_secret);
    let sign = format!("{:x}", sha2::Sha256::digest(sign_input.as_bytes()));

    let params = [
        ("q", text.as_str()),
        ("from", "auto"),
        ("to", resolved_lang.as_str()),
        ("appKey", app_key.as_str()),
        ("salt", salt.as_str()),
        ("sign", sign.as_str()),
        ("signType", "v3"),
        ("curtime", curtime.as_str()),
    ];

    let response = crate::infra::http::client()
        .post("https://openapi.youdao.com/api")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if response.status().is_success() {
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("JSON parsing error: {}", e))?;

        let translation = json
            .get("translation")
            .and_then(|t| t.as_array())
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if translation.is_empty() {
            let error_code = json
                .get("errorCode")
                .and_then(|c| c.as_str())
                .unwrap_or("unknown");
            let msg = match error_code {
                "202" => "签名检验失败，请检查 APP ID 和 APP KEY",
                "203" => "不支持的翻译方向",
                "205" => "请求的文本过长",
                "206" => "不支持的签名类型",
                "207" => "不支持的响应类型",
                "208" => "不支持的传输加密类型",
                "210" => "appKey已过期",
                "211" => "解密失败",
                "212" => "请求解密失败",
                "213" => "服务端queryString无法解析",
                "214" => "缺少必填的参数",
                "215" => "加密明文无效",
                "301" => "辞典查询失败",
                "302" => "翻译查询失败",
                "303" => "请求超过配额",
                "401" => "账户已欠费",
                "1001" => "无效的OCR类型",
                "1002" => "不支持OCR图片格式",
                "1003" => "OCR图片过大",
                "1004" => "OCR识别失败",
                "1101" | "1104" => "语音识别失败",
                "1102" => "语音超时",
                "1103" => "语音识别文本过长",
                "2000" | "2003" => "翻译文本过长",
                _ => return Err(format!("有道翻译错误: 未知错误: {}", error_code)),
            };
            return Err(format!("有道翻译错误: {}", msg));
        }

        Ok(TranslateResult {
            source: text,
            translation,
            engine: "有道翻译".to_string(),
        })
    } else {
        let status = response.status();
        let body_text = response.text().await.unwrap_or_default();
        Err(format!("HTTP Error: {} - {}", status, body_text))
    }
}

// ============================================================================
// AI Translation (OpenAI-compatible non-streaming) — 保留兼容
// ============================================================================

/// 中文占比是否超过阈值
fn is_chinese_dominant(text: &str) -> bool {
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

/// 检测源语言的中文名称（用于提示词模板变量替换）
fn detect_source_lang_name(text: &str) -> &'static str {
    if is_chinese_dominant(text) {
        "中文"
    } else {
        "英文"
    }
}

/// 智能方向：根据设置的目标语言和源文本自动判断翻译方向
/// 如果输入已是目标语言，则反转方向
fn smart_target_lang(text: &str, target_lang: &str) -> String {
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

/// 语言代码 → 中文名称
fn lang_code_to_name(code: &str) -> &str {
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

fn lang_code_to_name_en(code: &str) -> &str {
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

/// 构建翻译 system prompt
fn build_system_prompt(to_lang: &str) -> String {
    format!(
        "You are a professional translator. Translate the following text to {}.",
        lang_code_to_name_en(to_lang)
    )
}

/// 默认翻译提示词模板：user 消息只放原文
const DEFAULT_TRANSLATE_PROMPT: &str = "{text}";

/// 将提示词模板中的变量替换为实际值
/// 支持：{text}、{fromLang}、{toLang}
fn render_prompt(template: &str, text: &str, from_lang: &str, to_lang: &str) -> String {
    template
        .replace("{text}", text)
        .replace("{fromLang}", from_lang)
        .replace("{toLang}", to_lang)
}

/// 解析模板：空或纯空白时回退到默认模板
fn resolve_template<'a>(prompt: Option<&'a String>, fallback: &'a str) -> &'a str {
    match prompt {
        Some(t) if !t.trim().is_empty() => t,
        _ => fallback,
    }
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
    if text.trim().is_empty() {
        return Err("文本不能为空".to_string());
    }

    let (_scheme, safe_endpoint) = sse::validate_endpoint(&endpoint)?;

    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }

    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }

    // 智能方向：结合设置的目标语言和源文本自动判断
    let to_lang_code = smart_target_lang(&text, target_lang.as_deref().unwrap_or("zh"));
    let from_lang_name = detect_source_lang_name(&text);
    let to_lang_name = lang_code_to_name(&to_lang_code);

    // 渲染提示词
let template = resolve_template(prompt.as_ref(), DEFAULT_TRANSLATE_PROMPT);
    let rendered = render_prompt(template, &text, from_lang_name, to_lang_name);
    let system_content = build_system_prompt(&to_lang_code);

    let url = format!(
        "{}/chat/completions",
        safe_endpoint.trim_end_matches('/')
    );

    let body = serde_json::json!({
        "model": model.trim(),
        "messages": [
            {
                "role": "system",
                "content": system_content
            },
            {
                "role": "user",
                "content": rendered
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

    // 引擎名：从 endpoint 提取域名，或使用模型名
    let engine_label = sse::parse_scheme_host(&safe_endpoint)
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
        source: text,
        translation,
        engine: engine_label,
    })
}

// ============================================================================
// AI Translation (OpenAI-compatible streaming) — 新增流式版本
// ============================================================================

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
    if text.trim().is_empty() {
        return Err("文本不能为空".to_string());
    }

    let (_scheme, safe_endpoint) = sse::validate_endpoint(&endpoint)?;

    if model.trim().is_empty() {
        return Err("模型名称不能为空".into());
    }

    if api_key.trim().is_empty() {
        return Err("API Key 不能为空".into());
    }

    // 智能方向
    let to_lang_code = smart_target_lang(&text, target_lang.as_deref().unwrap_or("zh"));
    let from_lang_name = detect_source_lang_name(&text);
    let to_lang_name = lang_code_to_name(&to_lang_code);

    // 渲染提示词
let template = resolve_template(prompt.as_ref(), DEFAULT_TRANSLATE_PROMPT);
    let rendered = render_prompt(template, &text, from_lang_name, to_lang_name);
    let system_content = build_system_prompt(&to_lang_code);

    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: system_content,
        },
        ChatMessage {
            role: "user".to_string(),
            content: rendered,
        },
    ];

    sse::stream_openai_request(sse::StreamConfig {
        app: &app,
        endpoint: &safe_endpoint,
        api_key: &api_key,
        model: &model,
        messages,
        chunk_event: "translate-chunk",
        done_event: "translate-done",
        request_id: &request_id,
    })
    .await
}

// ============================================================================
// Utilities
// ============================================================================

/// Youdao v3 signature input truncation:
/// - ≤20 chars: use full text
/// - >20 chars: first 10 chars + char count + last 10 chars
fn truncate_for_sign(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 20 {
        return s.to_string();
    }
    let first: String = chars.iter().take(10).collect();
    let last: String = chars
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{}{}{}", first, chars.len(), last)
}

fn nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_nanos())
}

fn chrono_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    format!("{}", duration.as_secs())
}


pub fn init() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri::plugin::Builder::<tauri::Wry>::new("translate")
        .setup(|_app, _api| {
            #[cfg(target_os = "macos")]
            {
                use tauri::Emitter;
                crate::core::shortcut::register_shortcut_hook("translate", Box::new(|app, ctx| {
                    if ctx.window_hidden {
                        if let Ok(mut selected) = crate::core::shortcut::SELECTED_TEXT.lock() {
                            *selected = String::new();
                        }

                        let self_pid = std::process::id() as i32;
                        let target_pid = ctx.front_pid.filter(|&p| p != self_pid);

                        // AX 取词（不阻塞）+ clipboard snapshot + inject Cmd+C
                        let ax_text = crate::macos::text_selection::try_ax();
                        let snap = crate::macos::text_selection::snapshot_clipboard();
                        if ax_text.is_none() {
                            if let Some(pid) = target_pid {
                                crate::macos::text_selection::inject_copy(pid);
                            }
                        }

                        crate::macos::webkit_tuning::show_main(app);

                        let app_clone = app.clone();
                        std::thread::spawn(move || {
                            let text = if let Some(t) = ax_text {
                                t
                            } else {
                                crate::macos::text_selection::poll_clipboard(snap)
                            };
                            if let Ok(mut selected) = crate::core::shortcut::SELECTED_TEXT.lock() {
                                *selected = text.clone();
                            }
                            let _ = app_clone.emit("translate-text-ready", text);
                        });
                        return true;
                    }
                    // 窗口已可见：清空待翻译文本
                    let _ = app.emit("translate-text-ready", "");
                    false // 走默认行为
                }));
            }
            Ok(())
        })
        .build()
}
