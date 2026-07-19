# Shell rc 注入约定

Voidnix 扩展若需修改用户 `~/.zshrc`（或其它 shell rc），**必须**走框架统一模块：

`src-tauri/src/runtime/shell_rc.rs`

禁止各扩展私自 append / 自创 marker 风格。

## Marker 格式

单行注释，全行精确匹配：

```text
# voidnix <scope>
```

- **scope**：扩展 id（kebab-case），与 `extensions/<id>/` 一致  
  例：`zsh-autosuggestions`、`ai-providers`
- 识别：`line.trim() == "# voidnix <scope>"`  
  不用子串匹配，避免误伤用户自己的注释

## 块布局

与文件其余内容用**空行**分隔：

```text
# 用户其它内容

# voidnix <scope>
<body 行 1>
<body 行 2…>

# 用户其它内容
```

规则：

- marker 下一行起为 **body**：连续非空行；body 内**禁止空行**
- 写入时上下各留一个空行（`shell_rc::upsert_block` 负责）
- 多扩展共存：各 scope 独立块，互不覆盖

## API（Rust）

```rust
use crate::runtime::shell_rc;

// 幂等写入 / 更新（摘除后拼全文再比较，避免 body 收缩时 contains 误短路）
shell_rc::upsert_block(&zshrc_path, "ai-providers", body)?;

// 关闭扩展时摘除
shell_rc::remove_block(&zshrc_path, "zsh-autosuggestions")?;

// 探测
shell_rc::has_marker(&content, "ai-providers");
shell_rc::marker_line("ai-providers"); // → "# voidnix ai-providers"
shell_rc::quote_shell(path);           // POSIX 单引号
```

落盘：`*.voidnix-bak` 备份 + tmp+rename 原子写。

## 现有消费者

| scope                 | 扩展    | body 含义                                             |
| --------------------- | ------- | ----------------------------------------------------- |
| `zsh-autosuggestions` | 补全    | `export ZSH_AS_DIR=…; eval "$(… init)"`               |
| `ai-providers`        | AI 凭证 | `source ~/.config/voidnix/ai.env`（zshrc + zprofile） |

## 禁止

- `>>> voidnix-xxx >>>` / `<<< … <<<` 成对 marker（旧 ai 钩子已迁移清理；`filter_legacy_pair_markers` 缺 end 时整段保留，不删半个 rc）
- 无 marker 的裸 `export` / `source` 行
- 用扩展私有 backup 路径（统一 `*.voidnix-bak` + `atomic_write_rc`）
- body 中插入空行（会破坏摘除语义）

## 用户侧摘除

```bash
# 搜 marker
grep -n 'voidnix' ~/.zshrc
# 或关扩展（zsh-as 会 remove_block）；ai 钩子随 upsert 自愈
```
