# Agent 扩展

AI 助手扩展，整合对话、网络搜索、命令执行。原 `chat` 扩展重命名而来。

## 三层架构

```
┌──────────────────────────────────────────────────┐
│  Agent Loop（core/agent/loop_runner.rs）          │
│  loop { call_llm → parse tool_calls → 审批 →     │
│         执行 → scrub_secret → 回灌 role:tool →    │
│         下一轮 }，MAX_TURNS=10 防失控              │
├──────────────────────────────────────────────────┤
│  Tool Registry（core/agent/tool_registry.rs）     │
│  AgentTool trait: name/schema/requires_approval/  │
│                  call                             │
├──────────────────┬───────────────────────────────┤
│  web_search      │  run_command                   │
│  (Tavily→DDG→Wiki)│  (纵深防御 9 层)              │
└──────────────────┴───────────────────────────────┘
```

## 通信协议

`agent_run` 接收 `Channel<AgentEvent>`，立即返回 `session_id`，后台 spawn task 跑 loop。

事件流（`AgentEvent` 枚举，serde tag=type/content=data）：

| 事件               | 字段                     | 含义               |
| ------------------ | ------------------------ | ------------------ |
| `TextDelta`        | `text`                   | LLM 文本增量       |
| `ToolCallStart`    | `id`, `name`             | 工具调用开始       |
| `ToolCallArgs`     | `id`, `args`             | 完整参数（JSON）   |
| `ApprovalRequired` | `id`, `toolName`, `args` | 需用户审批         |
| `ToolResult`       | `id`, `ok`, `output`     | 工具结果（已净化） |
| `Completed`        | —                        | 本轮结束           |
| `Error`            | `message`                | 错误终止           |

specta 不支持 `Channel<AgentEvent>`（动态 JSON 不支持），前端手写类型 `src/types/agent.ts` + 裸 `invoke()`。

## Agent 工具

### web_search

Tavily 搜索（专为 AI 设计，返回含 `answer` 字段的结构化 JSON）。

- API：`POST api.tavily.com/search`，Bearer auth
- 免费额度：1000 次/月，无需信用卡
- 未配 API Key 时工具返回错误，引导用户去设置
- 未来可扩展 Brave / Serper 等（数据结构已预留 `type` 字段）

### run_command

纵深防御 9 层：

1. 命令白名单（用户在 settings 编辑；默认含 ls/cat/grep/sed/awk/cp/mv/git 等读+编辑命令；「执行并信任」按钮也会追加）
2. FORBIDDEN 硬禁（`osascript/sudo/sh/curl/wget/...`，不可被白名单覆盖）
3. 参数黑名单（`--exec/--upload-pack/-o/-C/...`）+ shell 元字符检测
4. 断路器（`rm -rf /` / `rm -rf ~` 即便 approved 也拦）
5. `env_clear()` + 白名单 env（防父进程 API key 进子进程）
6. cwd `canonicalize`（Phase 2 加 symlink 双检查）
7. `pre_exec` 设 rlimit（CPU 30s / DATA 512MB / NOFILE 64）
8. `tokio::time::timeout(30s)` + `kill_on_drop(true)`
9. 输出边读边截断 1 MiB + `scrub_secret` gitleaks 打码（最后一道兜底）

**三档审批**：

- 白名单内 + 无危险参数 → 直接执行
- FORBIDDEN 硬禁 → 直接拒
- 未知命令 / 危险参数（如 rm）→ 弹 `BaseDialog`，按钮「执行 / 执行并信任 / 取消」
- 「执行并信任」→ 持久化到 `agent.trustedCommands`（与 settings textarea 同步）

**默认白名单**（settings 初始值，用户可自由编辑增删）：

```
ls cat pwd echo head tail wc file stat date which whoami uname
find grep rg fd ag tree diff comm cmp md5sum shasum
mkdir touch cp mv ln tee truncate
sed awk sort uniq cut tr paste expand
jq yq bat
git
```

ls cat pwd echo head tail wc file stat date which whoami uname
find grep rg fd ag tree diff comm cmp md5sum shasum
mkdir touch cp mv ln tee truncate
sed awk sort uniq cut tr paste expand
jq yq bat
git

```

ls cat pwd echo head tail wc file stat date which whoami uname
find grep rg fd ag tree diff comm cmp md5sum shasum
mkdir touch cp mv ln tee truncate
sed awk sort uniq cut tr paste expand
jq yq bat
git

```

### System Prompt（harness）

每次 agent_run 注入 system message（messages[0]），由两部分组成：

1. **默认 harness**（Rust 端硬编码，`loop_runner.rs::DEFAULT_SYSTEM_PROMPT`）：描述 agent 角色、工具使用规则、安全约束、输出风格
2. **用户自定义**（可选）：在 settings 配置，追加为「用户自定义指令」段

用户自定义示例：

- 「始终用英文回答」
- 「优先使用 ripgrep 而非 grep」
- 「当前工作目录是 ~/Projects/myapp，使用 pnpm 而非 npm」

## 配置

settings.json 顶层分组：

```json
{
  "aiProviders": {
    "configs": [{ "id": "...", "endpoint": "...", "apiKey": "...", "models": [...] }],
    "activeProviderModelKey": "<id>::<model>"
  },
  "agent": {
    "searchProviders": [
      { "id": "default", "type": "tavily", "apiKey": "tvly-..." }
    ],
    "activeSearchProviderId": "default",
    "trustedCommands": ["docker", "npm", "node"],
    "systemPrompt": "始终用英文回答"
  }
}
```

工具调用始终启用（无开关）。搜索提供商与模型提供商同款多 provider 体系，可添加多个，通过 `activeSearchProviderId` 切换当前激活。

**v1 → v2 一次性迁移**（`migrateV1toV2`，幂等）：

- `chat.configs` → `aiProviders.configs`
- `chat.activeModelKey` → `aiProviders.activeProviderModelKey`
- `shortcuts.overrides.chat` → `shortcuts.overrides.agent`（值保留）
- `agent.tavilyKey`（早期单 key）→ `agent.searchProviders` 数组（type=tavily）

## 文件结构

```
extensions/agent/
├── index.ts               # module 注册（id 'agent'）
├── agent.ts               # useAgentChat composable（前端状态机）
├── View.vue               # part 渲染 + Approval 弹窗
├── Settings.vue           # Provider + Agent 配置
├── Actions.vue            # 模型切换 + 新会话
└── native/
    ├── mod.rs             # agent_run / agent_approve / agent_abort + Tier1 Plugin
    └── tools/
        ├── web_search.rs
        └── run_command.rs
```

## 扩展自定义 Agent 工具

未来其他扩展可 impl `AgentTool` trait，在 `agent_run` 内构造 `ToolRegistry` 时注册。当前 Phase 1 仅内置 `web_search` + `run_command`，Phase 2 考虑把 `clipboard` / `search` / `translate` 等扩展能力包成 agent tool。

## 已知限制（Phase 1）

- 无 OS 沙箱（macOS Seatbelt，Claude Code 同款）—— Phase 2 引入
- 无网络隔离（`curl`/`wget` 直接 FORBIDDEN，让 agent 用 `web_search`） —— Phase 2 引入
- 符号链接深层检查未实现 —— Phase 2 加
- 受信命令白名单 UI 只读（要编辑改 settings.json） —— Phase 2 加可视化编辑

## 测试

- Rust 单元测试：`cargo test --lib`（含 tool_calls_parser / secret_scrub / run_command / web_search 解析）
- 前端测试：`bun run test`（含 settings.ts 迁移逻辑 + 默认值）
