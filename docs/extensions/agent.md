# Agent 扩展

AI 助手扩展，整合对话、网络搜索、命令执行。

## 三层架构

```
┌──────────────────────────────────────────────────┐
│  Agent Loop（engine/loop_runner.rs）              │
│  loop { call_llm → parse tool_calls →            │
│         执行 → scrub_secret → 回灌 role:tool →    │
│         下一轮 }，max_turns 由 config 注入         │
├──────────────────────────────────────────────────┤
│  Tool Registry（engine/tool_registry.rs）         │
│  AgentTool trait: name/schema/call               │
├──────────────────┬───────────────────────────────┤
│  web_search      │  run_command                   │
│  (Tavily API)    │  (断路器 + 资源约束)          │
└──────────────────┴───────────────────────────────┘
```

Engine 代码在 `extensions/agent/native/engine/`（从框架层下沉，扩展自管）。

## 通信协议

`agent_run` 接收 `Channel<AgentEvent>`，立即返回 `session_id`，后台 spawn task 跑 loop。
loop 结束（含 error）时 `SessionRegistry::unregister` 清会话；用户 abort 走 `cancel`（token + abort handle）。

事件流（`AgentEvent` 枚举）：

- `TextDelta { text }`：LLM 文本增量
- `ToolCallStart { id, name }`：工具调用开始
- `ToolCallArgs { id, args }`：完整参数（JSON）
- `ToolResult { id, ok, output }`：工具结果（已净化；`run_command` 非 0 退出 `ok=false`，output 仍为完整命令输出）
- `Completed`：本轮结束
- `Error { message }`：错误终止（前端写入当前 assistant 气泡）

前端手写类型 `src/types/agent.ts`，经 `invoke(CMD.agentRun / CMD.agentAbort)` 调用。
`handleEvent` 内容按 `assistantId` 写气泡，且仅 `streaming` 时接受 delta/tool（abort/完成/错误 finalize 后拒绝晚到内容）；`status` / `sessionId` 仅当事件仍属当前 run（闭包 `runSessionId`）时才改，避免晚到 completed/error 踩踏新一轮。

## Agent 工具

### web_search

Tavily 搜索（专为 AI 设计，返回含 `answer` 字段的结构化 JSON）。

- API：`POST api.tavily.com/search`，Bearer auth
- 免费额度：1000 次/月，无需信用卡
- 未配 API Key 时工具返回错误，引导用户去设置

### run_command

命令无白名单/黑名单拦截——所有命令直接放行。仅以下机制兜底：

1. shell 元字符注入免疫：`tokio::process::Command` 不经 shell
2. 断路器（`rm -rf /` / `rm -rf ~` 等灾难性全局操作拦截，不可放宽）
3. `env_clear()` + 白名单 env（防父进程 API key 进子进程）
4. cwd `canonicalize`
5. `pre_exec` 设 rlimit（CPU / 内存 / 文件描述符上限由 config 配置）
6. `tokio::time::timeout` + `kill_on_drop(true)`（超时由 config 配置）
7. 输出边读边截断 + `scrub_secret` gitleaks 打码（最后一道兜底）

### System Prompt

`config.systemPrompt` 即 system message 本体（不再区分「默认 harness + 用户追加」）。

- 默认值在 `config.ts` 的 `defineConfig` 内（描述 agent 角色、工具规则、安全约束、输出风格），用户可全量改写。
- `agent_run` 收到后直接注入 `messages[0]`（空串跳过）；Rust 端不内置默认提示词。

## 配置

agent 配置通过 `defineConfig` 自管，持久化至 `extensions/agent/config.json`：

```typescript
// extensions/agent/config.ts
defineConfig('extensions/agent/config', {
  systemPrompt: '你是全能的 AI Agent…',
  searchProvider: { type: 'tavily', apiKey: '' },
  aiProviders: [{ id, endpoint: '', apiKey: '', models: [] }], // 多 provider + activeProviderModelKey 激活选择
  // 资源上限默认值（maxCpuSeconds/maxMemoryMb/maxOpenFiles/executionTimeout/maxOutputBytes/maxTurns）
  // BOUNDS 仅 CI 镜像 policy.rs，无 Settings UI；运行时 Rust clamp
})
```

AI Provider（endpoint/apiKey/models）由 agent 自管（与 translate 同构：各自 `config.ts` 维护独立 provider 列表，互不复用）。CRUD helpers：`addAiProvider()` / `updateAiProvider(id, partial)` / `removeAiProvider(id)`（保底 ≥1 项）/ `setActiveProviderModelKey(key)`；`activeProviderConfig` computed 解析激活项。

## 对话 UI

- **贴底滚动**：仅列表距底 < 24px 时 streaming 增量自动滚底；用户上翻阅读不打断。发送新消息时强制贴底。布局 watch 用轻量签名（条数 + streaming 长度）+ rAF 合并滚底，流式阶段不插值高度。
- **悬浮输入岛**：`agent-footer` absolute 贴底（左右/底 12），不占 flex 流；消息区铺满并可滚入其下；底 padding 固定预留输入岛（~112px），**不为**悬浮钮动态加高（避免显隐抖动）；`chrome-fade-bottom` 仍叠底软透。
- **工具结果**：`web_search` 成功展示 parsed answer/hits（button + 外链 icon，系统浏览器打开）；失败展示 `output` 错误串；`run_command` 等展示 `output` 原文。
- **状态 notice**（`AgentPart.type = 'notice'`，不进 LLM）：`error`（气泡 danger 底 + toast）/ `aborted`（muted「已中止」）；中止/错误时进行中工具标 `failed`。
- **未配置**：空态 +「去设置」打开 config 子视图。
- **悬浮操作**：输入框上方零宽中线锚点。滚底非贴底即显；中止仅输出中。两钮 absolute，solo=`translate -50%` 居中，pair=分居中线两侧（半槽 4px）；中止消失后滚底 translate 200ms 滑回正中。阴影 3 层插值 hover 抬升。点滚底 smooth；streaming/发送瞬时贴底。中止 aurora；进出场 200/150ms；中止走 `agentAbort`。
- **输入**：超长（>8192）截断并 toast；执行中 placeholder 提示可预写下一条。

## 文件结构

```
extensions/agent/
├── index.ts               # module 注册（id 'agent'）
├── config.ts              # defineConfig（systemPrompt/searchProvider/aiProviders + 资源默认值 + BOUNDS 仅 CI 镜像 + provider CRUD/active computed）
├── agent.ts               # useAgentChat composable（前端状态机）
├── view-logic.ts          # View 纯函数（streamView / showToolBody / markdown 等）
├── View.vue               # 布局 + 贴底滚动 + 悬浮操作 + notice
├── AgentTextPart.vue      # 流式/完成 markdown 文本 part
├── AgentToolStep.vue      # 工具步骤行 + hits/output
├── agent-step.css         # 思考/工具步骤共用样式
├── Settings.vue           # Provider + Agent 配置
├── Actions.vue            # 模型切换 + 新会话 + 设置
└── native/
    ├── mod.rs             # agent_run / agent_abort + Extension impl
    ├── policy.rs          # 资源上限 floor/cap 权威源（6 项 clamp）
    ├── engine/            # agent 引擎（从框架层下沉）
    │   ├── mod.rs         # AgentEvent 枚举
    │   ├── loop_runner.rs # 主循环（max_turns/system_prompt 由 LoopInput 注入）
    │   ├── cancellation.rs # SessionRegistry（CancellationToken）
    │   ├── trim.rs        # 历史消息裁剪
    │   ├── secret_scrub.rs # gitleaks 正则打码
    │   └── tool_registry.rs # AgentTool trait + ToolRegistry
    └── tools/
        ├── web_search.rs
        └── run_command.rs
```

## 测试

- Rust 单元测试：`cargo test --lib`（含 run_command 断路器测试 + policy 资源 clamp 测试 + LLM security 测试）
- 前端测试：`bun run test`
