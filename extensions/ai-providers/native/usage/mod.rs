//! 提供商用量的统一入口，供列表副标题/曲线消费。
//! 每提供商一个文件：获取协议差异大（认证方式 / SSRF 门禁 / 响应 shape），
//! 不做配置驱动的统一抽象，共享仅限真正共用的原语（fetch_text / JSON 宽松取值）。

pub mod deepseek;
pub mod zhipu;

/// GET 并返回 (status, body 文本)。空 body 与业务错误由调用方解释（各提供商语义不同）。
async fn fetch_text(
    client: &reqwest::Client,
    url: &str,
    headers: reqwest::header::HeaderMap,
) -> Result<(reqwest::StatusCode, String), String> {
    let res = client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("请求失败: {e}"))?;
    let status = res.status();
    let text = res.text().await.map_err(|e| format!("读取响应失败: {e}"))?;
    Ok((status, text))
}

/// JSON 数字宽松转 i64（兼容整数 / 浮点 / 数字字符串）。
fn json_i64(v: &serde_json::Value) -> i64 {
    if let Some(i) = v.as_i64() {
        return i;
    }
    if let Some(u) = v.as_u64() {
        return u.min(i64::MAX as u64) as i64;
    }
    if let Some(f) = v.as_f64() {
        return f as i64;
    }
    if let Some(s) = v.as_str() {
        if let Ok(i) = s.parse::<i64>() {
            return i;
        }
        if let Ok(f) = s.parse::<f64>() {
            return f as i64;
        }
    }
    0
}

/// JSON 数字宽松转 f64（兼容数字字符串；不匹配返回 None）。
fn json_f64(v: &serde_json::Value) -> Option<f64> {
    if let Some(f) = v.as_f64() {
        return Some(f);
    }
    if let Some(i) = v.as_i64() {
        return Some(i as f64);
    }
    if let Some(u) = v.as_u64() {
        return Some(u as f64);
    }
    if let Some(s) = v.as_str() {
        return s.parse().ok();
    }
    None
}
