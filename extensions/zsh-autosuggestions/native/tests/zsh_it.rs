//! init.zsh 集成测试：spawn `zsh -df` 加载**真实 binary 的 `init` 输出**（与
//! .zshrc 注入的 `eval "$(... init)"` 完全同路径），在真实解释器内执行断言。
//!
//! 覆盖纯 Rust 单测够不到的 zsh 侧逻辑：目录优先/上溯合并匹配、hooks 采集
//! （pwd 归属、5 字段 append、空白前缀跳过、histfile 不可读跳过）、cache 版本
//! 门禁（fail-closed）、reload 目录索引重建（含上一代段数组清理）、悬空索引键
//! 降级。fixtures 经 binary 的 `rebuild` 子命令生成（顺带覆盖 CLI 参数解析）。
//! zsh 不可用时跳过（CI 为 macOS 基线自带；防御非 macOS 环境）。

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// 编译产物 binary 路径（integration test 专属编译期注入）。
const BIN: &str = env!("CARGO_BIN_EXE_zsh-autosuggestions");

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

struct Fixture {
    dir: PathBuf,
    history: PathBuf,
}

/// 语料：git status ×4（压过 git push 成为全局 top-1）+ 各命令近期时间戳；
/// signals：proj 高频 cargo、proj/src 高频 npm start（语料外，验证目录维度
/// 不依赖 history）、docs 高频 make + 一条语料外命令。
fn make_fixture() -> Fixture {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("zsh-as-it-{}-{id}", std::process::id()));
    for sub in ["bin", "proj/src/deep", "docs", "else"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    let history = dir.join(".zsh_history");
    let signals = dir.join("signals.log");
    let cache = dir.join("index.zsh");

    let now = epoch_now();
    let hist_lines: Vec<String> = [
        (4, "git status", 500),
        (1, "git push", 450),
        (1, "ls -la", 100),
        (1, "cargo test", 90),
        (1, "cargo build", 80),
        (1, "make", 70),
        (1, "npm run dev", 60),
    ]
    .iter()
    .flat_map(|(count, cmd, ago)| {
        (0..*count)
            .map(move |i| format!(": {}:{};{cmd}\n", now - ago - i as i64, 0))
            .collect::<Vec<_>>()
    })
    .collect();
    std::fs::write(&history, hist_lines.concat()).unwrap();

    let sig_lines: Vec<String> = [
        (300, "proj", "cargo test"),
        (290, "proj", "cargo test"),
        (280, "proj", "cargo test"),
        (200, "proj", "cargo build"),
        (160, "proj/src", "npm start"),
        (150, "proj/src", "npm start"),
        (150, "docs", "make"),
        (140, "docs", "make"),
        (130, "docs", "not-in-history"),
    ]
    .iter()
    .map(|(ago, sub, cmd)| format!("{}\t0\t0\t{}\t{cmd}\n", now - ago, dir.join(sub).display()))
    .collect();
    std::fs::write(&signals, sig_lines.concat()).unwrap();

    // 经真实 CLI rebuild（覆盖 clap 参数解析），cache 落在 init 从 ZSH_AS_DIR
    // 派生的路径上
    let out = Command::new(BIN)
        .args([
            "rebuild",
            "--out",
            cache.to_str().unwrap(),
            "--history",
            history.to_str().unwrap(),
            "--signals",
            signals.to_str().unwrap(),
            "--half-life-days",
            "7",
            "--fail-penalty",
            "0.8",
        ])
        .output()
        .expect("spawn rebuild");
    assert!(
        out.status.success(),
        "rebuild 失败: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    Fixture { dir, history }
}

/// 断言脚本（$1=binary 路径，$2=fixture 目录）。结构与值断言逐一对应
/// init.zsh 行为，失败时输出 `FAIL: 名称 (got=[..] want=[..])` 并非零退出。
const SCRIPT: &str = r#"
pass=0; fail=0
ok()   { pass=$((pass+1)); print -r -- "  ok: $1" }
bad()  { fail=$((fail+1)); print -r -- "FAIL: $1 (got=[$2] want=[$3])" }
check() { [[ "$2" == "$3" ]] && ok "$1" || bad "$1" "$2" "$3" }
alt1() { local -a a=("${(@f)REPLY}"); print -r -- "${a[1]}" }
alt2() { local -a a=("${(@f)REPLY}"); print -r -- "${a[2]}" }

ZB="$1"; ZD="$2"
# 与 .zshrc 注入行完全同路径：eval binary init 输出
eval "$("$ZB" init)" 2>/dev/null
# zsh 调用 precmd 链中每个钩子前恢复链前 $?（实测 5.9），链中位置不影响
# exit 捕获，只断言注册存在
[[ -n "${precmd_functions[(r)_zsh_autosuggestions_precmd]}" ]] && ok "precmd 钩子已注册" || bad "precmd 钩子已注册" "missing" "registered"

# -- 目录优先（精确命中）--
cd "$ZD/proj"
_zsh_autosuggestions_match ""
check "空 buffer 目录 top-1" "$REPLY" "cargo test"
_zsh_autosuggestions_match "c"
check "前缀 c 首选来自目录" "$(alt1)" "cargo test"
check "前缀 c 次选目录补齐" "$(alt2)" "cargo build"
_zsh_autosuggestions_match "gi"
check "目录无命中回落全局" "$(alt1)" "git status"

# -- 上溯合并（近者优先）--
cd "$ZD/proj/src"
_zsh_autosuggestions_match ""
check "子目录空 buffer 取最近层级 top-1" "$REPLY" "npm start"
_zsh_autosuggestions_match "n"
check "子目录命中自身层级" "$(alt1)" "npm start"
_zsh_autosuggestions_match "c"
check "子目录命中祖先层级" "$(alt1)" "cargo test"
cd "$ZD/proj/src/deep"
_zsh_autosuggestions_match ""
check "未记录深层空 buffer 取最近祖先 top-1" "$REPLY" "npm start"
_zsh_autosuggestions_match "c"
check "更深层同样命中祖先" "$(alt1)" "cargo test"
_zsh_autosuggestions_match "m"
check "目录无命中回落全局 make" "$(alt1)" "make"

# -- 语料外命令（目录维度不依赖 history）--
cd "$ZD/docs"
_zsh_autosuggestions_match "not"
check "语料外命令目录内可建议" "$(alt1)" "not-in-history"
_zsh_autosuggestions_match "m"
check "docs 下 make 优先" "$(alt1)" "make"

# -- 未跟踪目录（无任何记录祖先）--
cd "$ZD/else"
_zsh_autosuggestions_match ""
check "未跟踪目录全局 top-1" "$REPLY" "git status"
_zsh_autosuggestions_match "c"
check "未跟踪目录走全局列表" "$(alt1)" "cargo build"

# -- hooks：pwd 归属 preexec 目录 + 5 字段 append --
cd "$ZD/proj"
_zsh_autosuggestions_preexec "cargo test"
cd "$ZD/else"
_zsh_autosuggestions_precmd
last=$(tail -1 "$ZSH_AS_SIGNALS")
IFS=$'\t' read -r f_ts f_exit f_state f_pwd f_cmd <<< "$last"
check "signals pwd 归属 preexec 目录" "$f_pwd" "$ZD/proj"
check "signals cmd" "$f_cmd" "cargo test"
check "signals exit=0" "$f_exit" "0"
check "signals state=0" "$f_state" "0"
[[ "$f_ts" == <16-> ]] && ok "ts 为 epoch 秒" || bad "ts 为 epoch 秒" "$f_ts" "<epoch>"

# -- 空白前缀命令跳过（隐私语义）--
n_before=$(wc -l < "$ZSH_AS_SIGNALS" | tr -d ' ')
_zsh_autosuggestions_preexec " secret-cmd"
_zsh_autosuggestions_precmd
n_after=$(wc -l < "$ZSH_AS_SIGNALS" | tr -d ' ')
check "空白前缀命令不记录" "$n_after" "$n_before"

# -- histfile 不可读跳过 append（防无消费方无限增长）--
HISTFILE_BAK="$HISTFILE"
HISTFILE="$ZD/no-such-history"
n_before=$(wc -l < "$ZSH_AS_SIGNALS" | tr -d ' ')
_zsh_autosuggestions_preexec "cargo build"
_zsh_autosuggestions_precmd
n_after=$(wc -l < "$ZSH_AS_SIGNALS" | tr -d ' ')
check "histfile 不可读不记录" "$n_after" "$n_before"
HISTFILE="$HISTFILE_BAK"

# -- 版本门禁：v1 拒绝 + fail-closed + 恢复 --
print -r -- "typeset -ga _zsh_autosuggestions_sorted=('old')" > "$ZD/v1.zsh"
print -r -- "typeset -gi _ZSH_AUTOSUGGESTIONS_IDX_VERSION=1" >> "$ZD/v1.zsh"
ZSH_AS_CACHE_BAK="$ZSH_AS_CACHE"
ZSH_AS_CACHE="$ZD/v1.zsh"
_zsh_autosuggestions_load_cache && bad "v1 cache 应拒绝" "ok" "fail" || ok "v1 cache 拒绝"
check "拒绝后版本归零" "$_ZSH_AUTOSUGGESTIONS_IDX_VERSION" "0"
_zsh_autosuggestions_match "old"
check "拒绝后 match 不出建议" "$REPLY" ""
ZSH_AS_CACHE="$ZSH_AS_CACHE_BAK"
_zsh_autosuggestions_load_cache
cd "$ZD/proj"
_zsh_autosuggestions_match ""
check "恢复后目录建议回归" "$REPLY" "cargo test"

# -- reload 索引重建：不残留 + 悬空键降级 --
check "reload 后 proj 在索引" "${_zsh_autosuggestions_dir_index[$ZD/proj]}" "d0"
[[ -z "${_zsh_autosuggestions_dir_index[/ghost]+x}" ]] && ok "ghost 不在索引" || bad "ghost 不在索引" "present" "absent"
_zsh_autosuggestions_dir_index[$ZD/else]=d99
cd "$ZD/else"
_zsh_autosuggestions_match "c"
check "悬空索引键回落全局" "$(alt1)" "cargo build"

# -- reload 清上一代段数组（段数收缩无孤儿常驻）--
_zsh_autosuggestions_dir_index[$ZD/gone]=d7
typeset -ga _zsh_autosuggestions_dir_d7=('stale')
_zsh_autosuggestions_load_cache
[[ -z "${_zsh_autosuggestions_dir_d7+x}" ]] && ok "上一代段数组随 reload 清除" || bad "上一代段数组随 reload 清除" "present" "absent"

print -r -- "zsh-it: $pass ok, $fail fail"
(( fail )) && exit 1
exit 0
"#;

#[test]
fn init_zsh_end_to_end() {
    // zsh 不可用则跳过（macOS 基线必在；防御非 macOS 环境跑测试）
    if !Command::new("zsh")
        .arg("-c")
        .arg("true")
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("跳过：环境中无 zsh");
        return;
    }

    let fx = make_fixture();
    let out = Command::new("zsh")
        .args(["-df", "-c", SCRIPT, "zsh-as-it", BIN])
        .arg(&fx.dir)
        .env("ZSH_AS_DIR", &fx.dir)
        .env("HISTFILE", &fx.history)
        .output()
        .expect("spawn zsh");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    // 汇总行是脚本跑到底的正向证据（防「全跳过仍通过」的假绿）
    let ok = out.status.success() && stdout.contains("zsh-it:") && !stdout.contains("FAIL:");
    if !ok {
        panic!(
            "init.zsh 集成断言失败（fixture: {}）:\nstdout:\n{stdout}\nstderr:\n{stderr}",
            fx.dir.display()
        );
    }
    print!("{}", stdout.trim_end());
    let _ = std::fs::remove_dir_all(&fx.dir);
}
