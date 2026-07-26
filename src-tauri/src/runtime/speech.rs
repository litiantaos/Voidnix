//! 语音朗读：macOS 自带 `say` CLI 驱动。
//!
//! tokio::process + oneshot 取消通道；朗读生命周期 = 进程生命周期。
//! speak_text 持锁贯穿「取消旧朗读 → spawn 新进程 → 注册取消信号」临界区，
//! 从根上消除并发覆盖导致的孤儿进程；select! 等进程退出或取消信号，无手摇轮询。
//!
//! 语音选择：`say -v '?'` 输出因 macOS 版本 / 已下载语音包而异（如 macOS 26 无
//! Ting-Ting/Amelie，中文为 Tingting）。故运行时查询一次已安装语音，按「经典语音 →
//! 主方言 → 任意同语种」优先级选取，跨机器自适应；全无则省略 -v 走系统默认。

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::Mutex;

use tokio::process::Command;
use tokio::sync::{oneshot, OnceCell};

use crate::runtime::lock_or_recover;

/// 当前朗读的取消信号（None = 无朗读）。speak_text 临界区内 take + send 取消旧朗读。
static CANCEL_TX: Mutex<Option<oneshot::Sender<()>>> = Mutex::new(None);

/// 取消当前朗读：取出发送端并发送信号（send 同步非阻塞，不持锁跨 await）。
fn cancel_current() {
    if let Some(tx) = lock_or_recover(&CANCEL_TX).take() {
        let _ = tx.send(());
    }
}

/// 已安装语音：name 集合 + 主方言 → 首个语音名。
struct VoiceIndex {
    names: std::collections::HashSet<String>,
    by_dialect: HashMap<String, String>,
}

static VOICES: OnceCell<VoiceIndex> = OnceCell::const_new();

/// 经典语音优先序（质量稳定，随系统预装或可下载）。macOS 26 上中文为 Tingting（无连字符）。
const PREFERRED: &[(&str, &[&str])] = &[
    ("zh", &["Tingting", "Sinji", "Meijia"]),
    ("en", &["Samantha", "Alex"]),
    ("ja", &["Kyoko"]),
    ("ko", &["Yuna"]),
    ("fr", &["Thomas", "Jacques"]),
    ("de", &["Anna", "Markus"]),
    ("es", &["Paulina", "Monica"]),
    ("ru", &["Milena", "Yuri"]),
    ("pt", &["Luciana", "Felipe"]),
    ("it", &["Alice", "Luca"]),
    ("th", &["Kanya"]),
    ("hi", &["Rishi"]),
    ("ar", &["Maged", "Tarik"]),
    ("vi", &["Minh"]),
];

/// 各语种主方言（简体中文 / 美式英文 等），经典语音缺失时回退取主方言首个。
const PRIMARY_DIALECT: &[(&str, &str)] = &[
    ("zh", "zh_CN"),
    ("en", "en_US"),
    ("ja", "ja_JP"),
    ("ko", "ko_KR"),
    ("fr", "fr_FR"),
    ("de", "de_DE"),
    ("es", "es_MX"),
    ("ru", "ru_RU"),
    ("pt", "pt_BR"),
    ("it", "it_IT"),
    ("th", "th_TH"),
    ("hi", "hi_IN"),
    ("ar", "ar_SA"),
    ("vi", "vi_VN"),
];

/// 解析 `say -v '?'` 输出为 (voice_name, lang_code) 列表。
/// 行格式：`VoiceName<spaces>langCode  # 示例`；langCode 锚点形如 en_US / zh_CN。
fn parse_voices(stdout: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let Some(idx) = find_lang_code(line) else {
            continue;
        };
        let name = line[..idx].trim().to_string();
        let lang = line[idx..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        if !name.is_empty() && !lang.is_empty() {
            out.push((name, lang));
        }
    }
    out
}

/// 定位行内 lang code 起始下标（小写 2-3 + _ + 大写 2-3，前置空白）。
fn find_lang_code(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 1;
    while i < b.len() {
        if !b[i - 1].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // 小写段
        let mut j = i;
        while j < b.len() && b[j].is_ascii_lowercase() {
            j += 1;
        }
        if j <= i || j >= b.len() || b[j] != b'_' {
            i += 1;
            continue;
        }
        // 大写段
        let mut k = j + 1;
        while k < b.len() && b[k].is_ascii_uppercase() {
            k += 1;
        }
        if k > j + 1 && (k == b.len() || b[k].is_ascii_whitespace()) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn load_voices() -> VoiceIndex {
    let mut names = std::collections::HashSet::new();
    let mut by_dialect: HashMap<String, String> = HashMap::new();
    // std::process::Command（同步）：仅经 spawn_blocking 在阻塞线程池调用，不占 async worker
    if let Ok(out) = std::process::Command::new("say")
        .arg("-v")
        .arg("?")
        .output()
    {
        for (name, lang) in parse_voices(&String::from_utf8_lossy(&out.stdout)) {
            names.insert(name.clone());
            by_dialect.entry(lang).or_insert(name);
        }
    }
    VoiceIndex { names, by_dialect }
}

/// 首次调用经 spawn_blocking 查询 say -v ?（避免 async 路径同步阻塞）；后续纯内存。
async fn voices() -> &'static VoiceIndex {
    VOICES
        .get_or_init(|| async {
            tokio::task::spawn_blocking(load_voices)
                .await
                .unwrap_or_else(|_| load_voices())
        })
        .await
}

/// 按 lang（zh/en/ja/...）选 say -v 语音名：经典 → 主方言 → 任意同语种；全无则 None。
async fn pick_voice(lang: &str) -> Option<String> {
    let idx = voices().await;
    let preferred = PREFERRED.iter().find(|(l, _)| *l == lang).map(|(_, v)| *v);
    if let Some(prefs) = preferred {
        for name in prefs {
            if idx.names.contains(*name) {
                return Some((*name).to_string());
            }
        }
    }
    if let Some(dialect) = PRIMARY_DIALECT
        .iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, d)| *d)
    {
        if let Some(name) = idx.by_dialect.get(dialect) {
            return Some(name.clone());
        }
    }
    // 任意同语种前缀（zh_* / en_* …）
    idx.by_dialect
        .iter()
        .find(|(l, _)| l.starts_with(lang))
        .map(|(_, n)| n.clone())
}

/// 朗读文本。异步：读完 / 被新朗读取代 / stop_speech 时 resolve。
/// text 经 arg 直传（无 shell 注入）；lang 为语种码，映射 say -v 语音。
///
/// 临界区（持锁）= 取消旧朗读 + spawn 新进程 + 注册取消信号，三步原子，
/// 从根上消除并发 speak 互相覆盖导致孤儿进程的竞态。
#[tauri::command]
pub async fn speak_text(text: String, lang: String) -> Result<(), String> {
    // 锁外选语音（首次经 spawn_blocking 查 say -v ?，不阻塞锁竞争者）
    let voice = pick_voice(&lang).await;

    // 临界区：cancel + spawn + 注册取消信号（fork+exec 微秒级，短暂持锁可接受）
    let (mut child, cancel_rx) = {
        let mut g = lock_or_recover(&CANCEL_TX);
        // 取消旧朗读（take 旧 sender + send 信号，send 同步非阻塞）
        if let Some(tx) = g.take() {
            let _ = tx.send(());
        }
        let mut cmd = Command::new("say");
        if let Some(v) = voice {
            cmd.arg("-v").arg(v);
        }
        cmd.arg(&text)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let child = cmd.spawn().map_err(|e| format!("say 启动失败: {e}"))?;
        let (tx, rx) = oneshot::channel::<()>();
        *g = Some(tx);
        (child, rx)
    };

    // 等进程自然结束或被取消（无轮询）
    tokio::select! {
        _ = child.wait() => Ok(()),
        _ = cancel_rx => {
            let _ = child.kill().await;
            Ok(())
        }
    }
}

/// 停止当前朗读：发送取消信号使进行中的 speak_text select 立即退出并 kill 子进程。
#[tauri::command]
pub fn stop_speech() {
    cancel_current();
}

#[cfg(test)]
mod tests {
    use super::parse_voices;

    #[test]
    fn parse_classic_and_localized_names() {
        let s = "Tingting               zh_CN    # 你好！我叫婷婷。\n\
                 Eddy (中文（中国大陆）)     zh_CN    # 你好！我叫Eddy。\n\
                 Samantha              en_US    # Hello! My name is Samantha.\n";
        let v = parse_voices(s);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], ("Tingting".into(), "zh_CN".into()));
        assert_eq!(v[1], ("Eddy (中文（中国大陆）)".into(), "zh_CN".into()));
        assert_eq!(v[2], ("Samantha".into(), "en_US".into()));
    }

    #[test]
    fn parse_skips_lines_without_lang_code() {
        let s = "no lang here\nSamantha en_US # hi\n";
        let v = parse_voices(s);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "Samantha");
    }
}
