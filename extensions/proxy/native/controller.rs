use serde_json::Value;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

/// mihomo controller 独立 HTTP 客户端（不经 http::client() 的 SSRF 防护——controller 固定
/// 127.0.0.1 本地回环，且 http::client 的重定向策略会拦截 localhost）。
///
/// pool_max_idle_per_host(0)：禁用空闲连接复用。mihomo 热重启后旧连接变 stale，
/// 复用会触发 "error sending request"（reqwest 不自动重试）。localhost 每次新建连接开销可忽略。
static CONTROLLER: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .pool_max_idle_per_host(0)
        .build()
        .expect("Failed to build mihomo controller client")
});

/// 测速探测 URL：经海外节点访问，用 Cloudflare generate_204（cp.cloudflare.com）——全球
/// anycast CDN 海外节点访问延迟最低最稳。**不用 gstatic**：gstatic.com 国内 DNS 污染到中国
/// 移动 IP（120.253.x.x），海外节点访问该污染 IP 跨海高延迟，致测速虚高（无论开关代理）。
/// cp.cloudflare.com 国内 DNS 解析得 Cloudflare 真实 IP（104.16.x.x），不污染。
const DELAY_TEST_URL: &str = "http://cp.cloudflare.com/generate_204";
const DELAY_TIMEOUT_MS: u64 = 5000;

/// GET /proxies → 完整代理树（含 selector/url-test 分组与各节点）。
pub async fn get_proxies(base: &str, secret: &str) -> Result<Value, String> {
    CONTROLLER
        .get(format!("{base}/proxies"))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| format!("controller 请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("controller 响应错误: {e}"))?
        .json::<Value>()
        .await
        .map_err(|e| format!("解析 proxies 失败: {e}"))
}

/// PUT /proxies/{group} → 在 selector 分组中选择节点。
pub async fn select_proxy(base: &str, secret: &str, group: &str, name: &str) -> Result<(), String> {
    let g = urlencoding::encode(group);
    CONTROLLER
        .put(format!("{base}/proxies/{g}"))
        .bearer_auth(secret)
        .json(&serde_json::json!({ "name": name }))
        .send()
        .await
        .map_err(|e| format!("切换节点失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("切换节点错误: {e}"))?;
    Ok(())
}

/// GET /proxies/{name}/delay → 延迟测速，返回 ms（失败/超时返回 0）。
pub async fn test_delay(base: &str, secret: &str, name: &str) -> Result<u32, String> {
    let n = urlencoding::encode(name);
    let test_url = urlencoding::encode(DELAY_TEST_URL);
    let url = format!("{base}/proxies/{n}/delay?url={test_url}&timeout={DELAY_TIMEOUT_MS}");
    let resp = CONTROLLER
        .get(&url)
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| format!("测速请求失败: {e}"))?;
    if !resp.status().is_success() {
        return Ok(0);
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| format!("解析测速响应失败: {e}"))?;
    Ok(v.get("delay")
        .and_then(|d| d.as_u64())
        .map(|d| d as u32)
        .unwrap_or(0))
}

/// PATCH /configs → 切换规则模式（rule | global | direct）。
pub async fn set_mode(base: &str, secret: &str, mode: &str) -> Result<(), String> {
    CONTROLLER
        .patch(format!("{base}/configs"))
        .bearer_auth(secret)
        .json(&serde_json::json!({ "mode": mode }))
        .send()
        .await
        .map_err(|e| format!("切换模式失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("切换模式错误: {e}"))?;
    Ok(())
}

/// PUT /configs {path} → 从磁盘 config.yaml 热重载（用于 root 运行时切换 tun，免重启免提权）。
pub async fn reload_config(base: &str, secret: &str, config_path: &str) -> Result<(), String> {
    let resp = CONTROLLER
        .put(format!("{base}/configs"))
        .bearer_auth(secret)
        .json(&serde_json::json!({ "path": config_path }))
        .send()
        .await
        .map_err(|e| format!("重载配置失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("重载配置错误: {status} {body}"));
    }
    Ok(())
}

/// GET /version 验证 secret 是否匹配运行中的 mihomo（true=200 匹配，false=401 不匹配）。
/// 连接失败（进程未就绪/已退出）返回 Err——调用方据此决定不清理（无法判定可控性）。
pub async fn check_auth(base: &str, secret: &str) -> Result<bool, String> {
    let resp = CONTROLLER
        .get(format!("{base}/version"))
        .bearer_auth(secret)
        .send()
        .await
        .map_err(|e| format!("controller 请求失败: {e}"))?;
    Ok(resp.status().is_success())
}

/// 轮询 GET /version 直到 controller 就绪或超时。
/// mihomo spawn 后需加载 geo/初始化，controller bind 有延迟；不等待会导致前端首查连接被拒。
pub async fn wait_ready(base: &str, secret: &str, timeout_ms: u64) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if CONTROLLER
            .get(format!("{base}/version"))
            .bearer_auth(secret)
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                "mihomo controller 启动超时，请检查端口是否被占用或核心是否崩溃".to_string(),
            );
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
