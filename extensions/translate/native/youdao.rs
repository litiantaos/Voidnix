use sha2::Digest;

use super::TranslateResult;
use super::lang_utils::smart_target_lang;

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

    let text = match text.char_indices().nth(5000) {
        Some((byte_idx, _)) => text[..byte_idx].to_string(),
        None => text,
    };

    let resolved_lang = smart_target_lang(&text, target_lang.as_deref().unwrap_or("zh"));
    let salt = nonce();
    let curtime = chrono_timestamp();
    let input = truncate_for_sign(&text);

    let sign_input = format!("{}{}{}{}{}", app_key, input, salt, curtime, app_secret);
    let sign = sha2::Sha256::digest(sign_input.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

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
