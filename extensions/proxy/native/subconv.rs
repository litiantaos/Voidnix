//! 通用格式订阅 → Clash YAML 转换。
//!
//! 订阅生态双格式并存：Clash YAML（完整配置）与「通用格式」（分享链接逐行拼接、
//! 整体 base64，源自 SS/V2Ray 时代；小火箭、v2rayN、NekoBox 等均以此为原生格式）。
//! 此前订阅链路只认 YAML：通用格式被 YAML 解析器当作纯量吞掉，`proxies` 计数为 0
//! 却不报错，用户只看到「已更新 0 个节点」的静默假成功。
//!
//! 本模块只做一件事：把通用格式的链接逐条翻成 mihomo `proxies` 条目，产出仅含
//! `proxies:` 的 YAML 文本（分组/规则仍走 subscription::merge_yaml 的默认链路，
//! 不在此重复实现）。未知协议行跳过；一行都解析不出才报错，由调用方给用户明确
//! 提示。节点重名（分享链接 fragment 全靠订阅方书写）追加序号去重——mihomo 对
//! 重名节点直接拒载 config，整个订阅不可用。

use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_norway::{Mapping, Value};
use std::collections::{HashMap, HashSet};

fn s(v: &str) -> Value {
    Value::String(v.to_string())
}

fn b(v: bool) -> Value {
    Value::Bool(v)
}

fn num(v: i64) -> Value {
    Value::from(v)
}

fn put(m: &mut Mapping, key: &str, val: Value) {
    m.insert(s(key), val);
}

fn put_str(m: &mut Mapping, key: &str, val: &str) {
    if !val.is_empty() {
        put(m, key, s(val));
    }
}

/// 转换结果：YAML 文本 + 解析/跳过行数（跳过数供调用方给用户可观测反馈——
/// 部分成功时「已更新 N 个节点（跳过 M 行）」比静默丢弃可信）。
pub struct Converted {
    pub yaml: String,
    pub skipped: usize,
}

/// 通用格式订阅文本（base64 整体编码或明文链接列表）→ 仅含 proxies 的 YAML 文本。
pub fn convert_to_yaml(text: &str) -> Result<Converted, String> {
    let body = decode_maybe_base64(text);
    let mut proxies: Vec<Value> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut skipped = 0usize;
    for line in body.lines() {
        let line = line.trim();
        // 空行是通用格式的常规排版（分组间隔/末尾换行），不计入跳过数——否则完全
        // 正常的订阅也会提示「跳过 N 行不支持的协议」
        if line.is_empty() {
            continue;
        }
        if !line.contains("://") {
            skipped += 1;
            continue;
        }
        match parse_link(line) {
            Some(p) => {
                let name = p
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let unique = dedup_name(&mut seen, name);
                if let Value::Mapping(ref m) = p {
                    // name 已确认非空（parse 侧保证），直接覆写去重后的唯一名
                    let mut with_name = m.clone();
                    put(&mut with_name, "name", s(&unique));
                    proxies.push(Value::Mapping(with_name));
                }
            }
            None => skipped += 1,
        }
    }
    if proxies.is_empty() {
        return Err(format!(
            "未解析出任何节点（跳过 {skipped} 行无法识别的内容）"
        ));
    }
    let mut root = Mapping::new();
    put(&mut root, "proxies", Value::Sequence(proxies));
    let yaml = serde_norway::to_string(&Value::Mapping(root))
        .map_err(|e| format!("序列化转换结果失败: {e}"))?;
    Ok(Converted { yaml, skipped })
}

/// 重名追加序号（首个同名后续 `-2`/`-3`…），保证 mihomo 不因重名拒载。
fn dedup_name(seen: &mut HashSet<String>, name: String) -> String {
    if seen.insert(name.clone()) {
        return name;
    }
    for n in 2.. {
        let candidate = format!("{name}-{n}");
        if seen.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("序号穷尽前必然命中唯一名")
}

/// 整体 base64 时先解码（容忍标准/URL-safe 字母表与有无 padding）；本身已是明文则原样返回。
fn decode_maybe_base64(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    let cleaned: String = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    for engine in [STANDARD, URL_SAFE, STANDARD_NO_PAD, URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(&cleaned) {
            if let Ok(decoded) = String::from_utf8(bytes) {
                if decoded.contains("://") {
                    return decoded;
                }
            }
        }
    }
    trimmed.to_string()
}

fn b64_decode_text(text: &str) -> Option<String> {
    let cleaned: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    for engine in [STANDARD, URL_SAFE, STANDARD_NO_PAD, URL_SAFE_NO_PAD] {
        if let Ok(bytes) = engine.decode(&cleaned) {
            if let Ok(decoded) = String::from_utf8(bytes) {
                return Some(decoded);
            }
        }
    }
    None
}

fn pct_decode(text: &str) -> String {
    urlencoding::decode(text)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| text.to_string())
}

/// 一条分享链接的拆解结果（query 已 percent-decode 进 params）。
struct Link {
    scheme: String,
    userinfo: String,
    host: String,
    port: u16,
    params: HashMap<String, String>,
    name: String,
}

fn parse_link(line: &str) -> Option<Value> {
    let (scheme, rest) = line.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    // vmess 的载荷是 base64 JSON，没有 userinfo/query 结构，单独处理
    if scheme == "vmess" {
        return parse_vmess(rest);
    }
    let (no_frag, frag) = match rest.split_once('#') {
        Some((a, f)) => (a, pct_decode(f)),
        None => (rest, String::new()),
    };
    let (authority, query) = match no_frag.split_once('?') {
        Some((a, q)) => (a, q),
        None => (no_frag, ""),
    };
    let mut params: HashMap<String, String> = HashMap::new();
    for kv in query.split('&') {
        if let Some((k, v)) = kv.split_once('=') {
            params.insert(pct_decode(k), pct_decode(v));
        }
    }
    // authority 里可能带 /path（ws path 未走 query 的旧写法），authority 本体只取首段
    let authority = authority.split('/').next().unwrap_or("");
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((u, h)) => (pct_decode(u), h),
        None => (String::new(), authority),
    };
    // 分享链接几乎总带显式端口；各协议事实默认均为 443，仅作缺省兜底
    let (host, port) = parse_hostport(hostport, 443)?;
    let name = if frag.is_empty() {
        format!("{scheme}-{host}:{port}")
    } else {
        frag
    };
    let link = Link {
        scheme,
        userinfo,
        host,
        port,
        params,
        name,
    };
    match link.scheme.as_str() {
        "vless" => Some(parse_vless(&link)),
        "hysteria2" | "hy2" => Some(parse_hysteria2(&link)),
        "trojan" => Some(parse_trojan(&link)),
        "ss" => parse_ss(&link, line),
        _ => None,
    }
}

fn parse_hostport(hostport: &str, default: u16) -> Option<(String, u16)> {
    if let Some(rest) = hostport.strip_prefix('[') {
        // IPv6 字面量 [::1]:443
        let (host, tail) = rest.split_once(']')?;
        let port = tail
            .strip_prefix(':')
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(default);
        return Some((host.to_string(), port));
    }
    match hostport.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => Some((h.to_string(), port)),
            Err(_) => Some((hostport.to_string(), default)),
        },
        None => Some((hostport.to_string(), default)),
    }
}

/// alpn 参数（逗号分隔，如 `h2,http/1.1`）→ mihomo 序列字段。缺省不写。
fn put_alpn(m: &mut Mapping, link: &Link) {
    if let Some(alpn) = link.params.get("alpn") {
        let list: Vec<Value> = alpn
            .split(',')
            .map(|a| s(a.trim()))
            .filter(|a| !a.as_str().unwrap_or("").is_empty())
            .collect();
        if !list.is_empty() {
            put(m, "alpn", Value::Sequence(list));
        }
    }
}

/// 自签证书放行参数 → skip-cert-verify。各协议参数名不一（vless 常见 `allowInsecure`/`insecure`、
/// trojan `allowInsecure`、hy2 `insecure`），keys 按协议传入；缺省不写。缺失只表现为握手失败
/// 的测速超时，难定位——宁可字段宽松映射。
fn put_insecure(m: &mut Mapping, link: &Link, keys: &[&str]) {
    let insecure = keys.iter().any(|k| {
        link.params
            .get(*k)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    });
    if insecure {
        put(m, "skip-cert-verify", b(true));
    }
}

fn base_proxy(link: &Link, ty: &str) -> Mapping {
    let mut m = Mapping::new();
    put_str(&mut m, "name", &link.name);
    put_str(&mut m, "type", ty);
    put_str(&mut m, "server", &link.host);
    put(&mut m, "port", num(i64::from(link.port)));
    m
}

/// ws/grpc 传输层选项（vless/trojan 共用 mihomo 字段形态）。
fn put_transport(m: &mut Mapping, link: &Link) {
    let network = link.params.get("type").map(|v| v.as_str()).unwrap_or("tcp");
    put_str(m, "network", network);
    match network {
        "ws" => {
            let mut ws = Mapping::new();
            put_str(
                &mut ws,
                "path",
                link.params.get("path").map(|v| v.as_str()).unwrap_or("/"),
            );
            if let Some(host) = link.params.get("host") {
                let mut headers = Mapping::new();
                put_str(&mut headers, "Host", host);
                put(&mut ws, "headers", Value::Mapping(headers));
            }
            put(m, "ws-opts", Value::Mapping(ws));
        }
        "grpc" => {
            let mut grpc = Mapping::new();
            put_str(
                &mut grpc,
                "grpc-service-name",
                link.params
                    .get("serviceName")
                    .map(|v| v.as_str())
                    .unwrap_or(""),
            );
            put(m, "grpc-opts", Value::Mapping(grpc));
        }
        _ => {}
    }
}

fn parse_vless(link: &Link) -> Value {
    let mut m = base_proxy(link, "vless");
    put_str(&mut m, "uuid", &link.userinfo);
    put(&mut m, "udp", b(true));
    if let Some(flow) = link.params.get("flow") {
        put_str(&mut m, "flow", flow);
    }
    put_transport(&mut m, link);
    let security = link
        .params
        .get("security")
        .map(|v| v.as_str())
        .unwrap_or("");
    if security == "reality" || security == "tls" {
        put(&mut m, "tls", b(true));
        if let Some(sni) = link.params.get("sni") {
            put_str(&mut m, "servername", sni);
        }
        if let Some(fp) = link.params.get("fp") {
            put_str(&mut m, "client-fingerprint", fp);
        }
        put_alpn(&mut m, link);
        put_insecure(&mut m, link, &["allowInsecure", "insecure"]);
        if security == "reality" {
            let mut ro = Mapping::new();
            if let Some(pbk) = link.params.get("pbk") {
                put_str(&mut ro, "public-key", pbk);
            }
            if let Some(sid) = link.params.get("sid") {
                put_str(&mut ro, "short-id", sid);
            }
            put(&mut m, "reality-opts", Value::Mapping(ro));
        }
    }
    Value::Mapping(m)
}

fn parse_hysteria2(link: &Link) -> Value {
    let mut m = base_proxy(link, "hysteria2");
    put_str(&mut m, "password", &link.userinfo);
    put(&mut m, "udp", b(true));
    if let Some(sni) = link.params.get("sni") {
        put_str(&mut m, "sni", sni);
    }
    put_alpn(&mut m, link);
    put_insecure(&mut m, link, &["insecure"]);
    if let Some(obfs) = link.params.get("obfs") {
        put_str(&mut m, "obfs", obfs);
    }
    if let Some(pwd) = link.params.get("obfs-password") {
        put_str(&mut m, "obfs-password", pwd);
    }
    if let Some(ports) = link.params.get("ports") {
        put_str(&mut m, "ports", ports);
    }
    // 带宽声明：分享链接的 up/down 是 Mbps 裸数字（hysteria2 URI 约定），而 mihomo
    // 的 up/down 是带宽字符串（"50 Mbps"）——裸数字直接透传会被当成 50 bps 把连接限死
    put_bandwidth(&mut m, link, "up");
    put_bandwidth(&mut m, link, "down");
    Value::Mapping(m)
}

/// up/down 带宽参数归一：裸数字按 Mbps 补齐单位（hysteria2 URI 约定），已带单位的原样保留。
fn put_bandwidth(m: &mut Mapping, link: &Link, key: &str) {
    let Some(raw) = link.params.get(key) else {
        return;
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return;
    }
    let numeric = raw.chars().all(|c| c.is_ascii_digit() || c == '.');
    let value = if numeric {
        format!("{raw} Mbps")
    } else {
        raw.to_string()
    };
    put_str(m, key, &value);
}

fn parse_trojan(link: &Link) -> Value {
    let mut m = base_proxy(link, "trojan");
    put_str(&mut m, "password", &link.userinfo);
    put(&mut m, "udp", b(true));
    if let Some(sni) = link.params.get("sni").or_else(|| link.params.get("peer")) {
        put_str(&mut m, "sni", sni);
    }
    put_alpn(&mut m, link);
    put_insecure(&mut m, link, &["allowInsecure", "insecure"]);
    put_transport(&mut m, link);
    Value::Mapping(m)
}

fn parse_vmess(rest: &str) -> Option<Value> {
    let payload = rest.split(['#', '?']).next().unwrap_or("");
    let json_text = b64_decode_text(payload)?;
    let j: serde_json::Value = serde_json::from_str(&json_text).ok()?;
    let get = |k: &str| -> String {
        match j.get(k) {
            Some(serde_json::Value::String(v)) => v.clone(),
            Some(serde_json::Value::Number(v)) => v.to_string(),
            _ => String::new(),
        }
    };
    let port: u16 = get("port").parse().ok()?;
    let mut m = Mapping::new();
    let name = {
        let ps = get("ps");
        if ps.is_empty() {
            format!("vmess-{}:{port}", get("add"))
        } else {
            ps
        }
    };
    put_str(&mut m, "name", &name);
    put_str(&mut m, "type", "vmess");
    put_str(&mut m, "server", &get("add"));
    put(&mut m, "port", num(i64::from(port)));
    put_str(&mut m, "uuid", &get("id"));
    let aid: i64 = get("aid").parse().unwrap_or(0);
    put(&mut m, "alterId", num(aid));
    let scy = get("scy");
    put_str(&mut m, "cipher", if scy.is_empty() { "auto" } else { &scy });
    put(&mut m, "udp", b(true));
    let net = {
        let n = get("net");
        if n.is_empty() {
            "tcp".to_string()
        } else {
            n
        }
    };
    put_str(&mut m, "network", &net);
    match net.as_str() {
        "ws" => {
            let mut ws = Mapping::new();
            let path = get("path");
            put_str(&mut ws, "path", if path.is_empty() { "/" } else { &path });
            let host = get("host");
            if !host.is_empty() {
                let mut headers = Mapping::new();
                put_str(&mut headers, "Host", &host);
                put(&mut ws, "headers", Value::Mapping(headers));
            }
            put(&mut m, "ws-opts", Value::Mapping(ws));
        }
        "grpc" => {
            let mut grpc = Mapping::new();
            put_str(&mut grpc, "grpc-service-name", &get("path"));
            put(&mut m, "grpc-opts", Value::Mapping(grpc));
        }
        _ => {}
    }
    let tls = get("tls");
    if tls == "tls" || tls == "xtls" {
        put(&mut m, "tls", b(true));
        let sni = get("sni");
        if !sni.is_empty() {
            put_str(&mut m, "servername", &sni);
        }
        let fp = get("fp");
        if !fp.is_empty() {
            put_str(&mut m, "client-fingerprint", &fp);
        }
        let alpn = get("alpn");
        if !alpn.is_empty() {
            let list: Vec<Value> = alpn
                .split(',')
                .map(|a| s(a.trim()))
                .filter(|a| !a.as_str().unwrap_or("").is_empty())
                .collect();
            if !list.is_empty() {
                put(&mut m, "alpn", Value::Sequence(list));
            }
        }
    }
    Some(Value::Mapping(m))
}

fn parse_ss(link: &Link, raw_line: &str) -> Option<Value> {
    // SIP002：ss://base64(method:password)@host:port / ss://method:password@host:port
    // 旧式整体编码：ss://base64(method:password@host:port)（无 @ 分隔）
    let (method, password, host, port) = if !link.userinfo.is_empty() {
        let decoded = b64_decode_text(&link.userinfo).unwrap_or_else(|| link.userinfo.clone());
        let (method, password) = decoded.split_once(':')?;
        (
            method.to_string(),
            password.to_string(),
            link.host.clone(),
            link.port,
        )
    } else {
        let payload = raw_line
            .split_once("://")?
            .1
            .split(['#', '?'])
            .next()
            .unwrap_or("");
        let decoded = b64_decode_text(payload)?;
        let (cred, hostport) = decoded.rsplit_once('@')?;
        let (method, password) = cred.split_once(':')?;
        let (host, port) = parse_hostport(hostport, 443)?;
        (method.to_string(), password.to_string(), host, port)
    };
    let mut m = Mapping::new();
    put_str(&mut m, "name", &link.name);
    put_str(&mut m, "type", "ss");
    put_str(&mut m, "server", &host);
    put(&mut m, "port", num(i64::from(port)));
    put_str(&mut m, "cipher", &method);
    put_str(&mut m, "password", &password);
    put(&mut m, "udp", b(true));
    // plugin 参数（SIP002）：obfs-local;obfs=http;obfs-host=x → mihomo 的 obfs 插件
    if let Some(plugin) = link.params.get("plugin") {
        let mut parts = plugin.split(';');
        let plugin_id = parts.next().unwrap_or("");
        let mihomo_id = if plugin_id == "obfs-local" {
            "obfs"
        } else {
            plugin_id
        };
        put_str(&mut m, "plugin", mihomo_id);
        let mut opts = Mapping::new();
        for kv in parts {
            if let Some((k, v)) = kv.split_once('=') {
                let key = match k {
                    "obfs" => "mode",
                    "obfs-host" => "host",
                    other => other,
                };
                put_str(&mut opts, key, v);
            }
        }
        if !opts.is_empty() {
            put(&mut m, "plugin-opts", Value::Mapping(opts));
        }
    }
    Some(Value::Mapping(m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD as B64;

    const VLESS: &str = "vless://11111111-2222-3333-4444-555555555555@192.0.2.1:443?encryption=none&flow=xtls-rprx-vision&security=reality&sni=www.example.com&fp=chrome&pbk=PUBKEY&sid=abcd&type=tcp#node-a";
    const HY2: &str = "hysteria2://secretpw@192.0.2.2:443?insecure=1&sni=www.example.com#node-b";

    fn proxies_of(yaml: &str) -> Vec<Value> {
        let v: Value = serde_norway::from_str(yaml).unwrap();
        v.get("proxies")
            .and_then(|p| p.as_sequence())
            .unwrap()
            .clone()
    }

    #[test]
    fn converts_base64_blob() {
        let blob = B64.encode(format!("{VLESS}\n{HY2}\n"));
        let out = convert_to_yaml(&blob).unwrap();
        let ps = proxies_of(&out.yaml);
        assert_eq!(ps.len(), 2);
        let a = &ps[0];
        assert_eq!(a.get("name").and_then(|v| v.as_str()), Some("node-a"));
        assert_eq!(a.get("type").and_then(|v| v.as_str()), Some("vless"));
        let ro = a.get("reality-opts").unwrap();
        assert_eq!(
            ro.get("public-key").and_then(|v| v.as_str()),
            Some("PUBKEY")
        );
        assert_eq!(ro.get("short-id").and_then(|v| v.as_str()), Some("abcd"));
        let bnode = &ps[1];
        assert_eq!(
            bnode.get("type").and_then(|v| v.as_str()),
            Some("hysteria2")
        );
        assert_eq!(
            bnode.get("skip-cert-verify").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn converts_plain_link_list() {
        let out = convert_to_yaml(&format!("{VLESS}\n{HY2}\n")).unwrap();
        assert_eq!(proxies_of(&out.yaml).len(), 2);
    }

    #[test]
    fn converts_vmess_and_trojan_and_ss() {
        let vmess_json = r#"{"v":"2","ps":"vm-1","add":"192.0.2.3","port":"443","id":"11111111-2222-3333-4444-555555555555","aid":"0","scy":"auto","net":"ws","type":"none","host":"cdn.example.com","path":"/ws","tls":"tls","sni":"cdn.example.com"}"#;
        let vmess = format!("vmess://{}", B64.encode(vmess_json));
        let trojan = "trojan://pw123@192.0.2.4:443?sni=t.example.com#tj-1";
        let ss_cred = B64.encode("aes-256-gcm:sspw");
        let ss = format!("ss://{ss_cred}@192.0.2.5:8388#ss-1");
        let out = convert_to_yaml(&format!("{vmess}\n{trojan}\n{ss}\n")).unwrap();
        let ps = proxies_of(&out.yaml);
        assert_eq!(ps.len(), 3);
        assert_eq!(ps[0].get("type").and_then(|v| v.as_str()), Some("vmess"));
        assert_eq!(ps[0].get("network").and_then(|v| v.as_str()), Some("ws"));
        assert_eq!(ps[1].get("type").and_then(|v| v.as_str()), Some("trojan"));
        assert_eq!(
            ps[2].get("cipher").and_then(|v| v.as_str()),
            Some("aes-256-gcm")
        );
        assert_eq!(ps[2].get("password").and_then(|v| v.as_str()), Some("sspw"));
    }

    #[test]
    fn duplicate_names_get_suffixed() {
        // mihomo 对重名节点拒载 config，转换层必须去重
        let dup = "trojan://pw@192.0.2.6:443#same-name";
        let dup2 = "trojan://pw2@192.0.2.7:8443#same-name";
        let out = convert_to_yaml(&format!("{dup}\n{dup2}\n")).unwrap();
        let ps = proxies_of(&out.yaml);
        assert_eq!(ps.len(), 2);
        assert_eq!(
            ps[0].get("name").and_then(|v| v.as_str()),
            Some("same-name")
        );
        assert_eq!(
            ps[1].get("name").and_then(|v| v.as_str()),
            Some("same-name-2")
        );
    }

    #[test]
    fn maps_alpn_and_self_signed_cert_flags() {
        // vless 自签节点：allowInsecure/insecure → skip-cert-verify；alpn 逗号切数组
        let vless = "vless://11111111-2222-3333-4444-555555555555@192.0.2.8:443?security=tls&allowInsecure=1&alpn=h2,http/1.1#self-signed";
        // trojan 的 insecure 变体同样放行
        let trojan = "trojan://pw@192.0.2.4:443?insecure=true#tj-2";
        let out = convert_to_yaml(&format!("{vless}\n{trojan}\n")).unwrap();
        let ps = proxies_of(&out.yaml);
        for p in &ps {
            assert_eq!(
                p.get("skip-cert-verify").and_then(|v| v.as_bool()),
                Some(true)
            );
        }
        let alpn = ps[0].get("alpn").and_then(|v| v.as_sequence()).unwrap();
        let alpn: Vec<&str> = alpn.iter().filter_map(|a| a.as_str()).collect();
        assert_eq!(alpn, vec!["h2", "http/1.1"]);
    }

    #[test]
    fn hysteria2_bandwidth_gets_mbps_unit() {
        // URI 约定 up/down 为 Mbps 裸数字；mihomo 需带宽字符串，否则按 bps 限速
        let with_numbers = "hysteria2://pw@192.0.2.10:443?up=50&down=100#bw-a";
        let out = convert_to_yaml(with_numbers).unwrap();
        let ps = proxies_of(&out.yaml);
        assert_eq!(ps[0].get("up").and_then(|v| v.as_str()), Some("50 Mbps"));
        assert_eq!(ps[0].get("down").and_then(|v| v.as_str()), Some("100 Mbps"));
        // 已带单位的原样保留
        let with_unit = "hysteria2://pw@192.0.2.11:443?up=50%20Mbps#bw-b";
        let out = convert_to_yaml(with_unit).unwrap();
        let ps = proxies_of(&out.yaml);
        assert_eq!(ps[0].get("up").and_then(|v| v.as_str()), Some("50 Mbps"));
    }

    #[test]
    fn garbage_and_unknown_schemes_error() {
        assert!(convert_to_yaml("hello world, not a subscription").is_err());
        assert!(convert_to_yaml("unknown://x@192.0.2.9:443#n").is_err());
        // 部分可识别时保留可识别部分
        let out = convert_to_yaml(&format!("unknown://x\n{VLESS}\n")).unwrap();
        assert_eq!(proxies_of(&out.yaml).len(), 1);
        // 部分成功时跳过行数如实上报（供前端提示）
        assert_eq!(out.skipped, 1);
    }

    #[test]
    fn blank_lines_do_not_count_as_skipped() {
        // 分享链接订阅常有分组空行：空行不是「不支持的协议」，不应触发跳过提示
        let body = format!(
            "
{VLESS}

{HY2}

"
        );
        let out = convert_to_yaml(&body).unwrap();
        assert_eq!(out.skipped, 0, "空行不应计入 skipped");
    }
}
