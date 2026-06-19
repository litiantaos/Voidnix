use reqwest::Client;
use std::sync::LazyLock;
use std::time::Duration;

static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .timeout(Duration::from_secs(120))
        .pool_max_idle_per_host(10)
        .build()
        .expect("Failed to build HTTP client")
});

pub fn client() -> &'static Client {
    &HTTP_CLIENT
}

/// 通用 HTTP GET：绕过 webview 的 UA/Referer 反爬与 CORS 限制。
/// 纯 TS 扩展（ip/currency）等无 native 的消费者使用；返回响应体文本，前端 JSON.parse。
#[tauri::command]
pub async fn http_get(url: String) -> Result<String, String> {
    HTTP_CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())
}
