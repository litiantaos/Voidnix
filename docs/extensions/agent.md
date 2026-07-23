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

`handleEvent` 写入规则：

- **内容写入**：按 `assistantId` 写气泡
- **delta/tool 接受窗口**：仅 `streaming` 时接受（abort/完成/错误 finalize 后拒绝晚到内容）
- **status / sessionId 守卫**：仅当事件仍属当前 run（闭包 `runSessionId`）时才改，避免晚到 completed/error 踩踏新一轮

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
  // 资源上限默认值（maxCpuSeconds/maxMemoryMb/maxOpenFiles/executionTimeout/maxOutputBytes/maxTurns）
  // BOUNDS 仅 CI 镜像 policy.rs，无 Settings UI；运行时 Rust clamp
})
```

### 凭证选用

- **凭证中枢**：AI 凭证条目上收至框架级中枢（`@/runtime/ai-providers`），本扩展**自选** Key×模型
- **选用串**：`providerModelKey` = `providerId::keyId::model`（旧式两段 `providerId::model` 读时规范为三段）
- **多 Key 文案**：选项/触发文案为 `模型 · 备注`
- **UI/解析**：走 `effectiveProviderModelKey`（读时过滤悬空 + 规范 keyId，热路径不写回）
- **默认值**：无显式选用时默认首个可用提供商（endpoint + 非空 key + 模型齐备，读时推导不写回）
- **发起对话**：`resolveAgentCredentials()` 按有效选用解析，中枢无可用提供商则提示配置
- **冷 prune**：启动 `pruneAgentSelection` 写回（悬空清空 / 两段补 keyId）
- **迁移**：旧 `activeProviderModelKey` 一次性迁入
- 详见 [ai-providers.md](./ai-providers.md)

## 对话 UI

### 贴底滚动

- **自动滚底条件**：仅列表距底 < 24px 时 streaming 增量自动滚底
- **上翻不打断**：用户上翻阅读时不自动滚底
- **发送强制贴底**：发送新消息时强制贴底
- **布局 watch**：用轻量签名（条数 + streaming 长度）+ rAF 合并滚底，流式阶段不插值高度

### 悬浮输入岛

- **布局**：`agent-footer` absolute 贴底（左右/底 12），不占 flex 流
- **消息区**：铺满并可滚入 footer 下方
- **底部预留**：滚动区 `padding-bottom` + `chrome-fade-bottom` 高度，由 ResizeObserver 跟踪 footer 实际高度动态驱动（`--agent-footer-reserve`）
- **恒定间距**：末条消息距输入岛恒为 `--space`
- **渐隐贴合**：渐隐精确贴合 footer 区域
- **同步增长**：textarea 自动撑高时三者（padding / 渐隐 / textarea）同步增长

### 思考模式

`AgentPart.type = 'reasoning'`：

- **来源**：LLM 思考模式输出（`reasoning_content`，DeepSeek-R1 / 智谱 GLM / Kimi 等）流式累积成 reasoning part
- **展示**：三行省略（sparkling 图标 + 「思考」标签 + secondary 文本）
- **不回灌 LLM**：`toLlmMessages` 只取 text，reasoning 仅 UI 可见
- **多轮分段**：每轮独立成段（reasoningDelta 累积到最后一个 reasoning part 或新建）

### 工具结果

- **web_search 成功**：展示 answer 摘要（三行省略）
- **web_search 失败**：展示 `output` 错误串
- **run_command 等**：展示 `output` 原文

### 状态 notice

`AgentPart.type = 'notice'`，不进 LLM：

- **error**：气泡 danger 底 + toast
- **aborted**：muted「已中止」
- **副作用**：中止/错误时进行中工具标 `failed`

### 未配置

- 空态 +「去设置」打开 config 子视图

### 悬浮操作

- **锚点**：输入框上方零宽中线锚点
- **滚底按钮显隐**：滚底非贴底即显
- **中止按钮显隐**：仅输出中显示
- **双钮布局**：两钮 absolute；solo = `translate -50%` 居中；pair = 分居中线两侧（半槽 4px）
- **单→双回正**：中止消失后滚底按钮 `translate` 200ms 滑回正中
- **滚底点击**：smooth；streaming/发送时瞬时贴底
- **中止样式**：aurora
- **进出场**：200/150ms
- **中止调用**：走 `agentAbort`
- **阴影**：3 层插值 hover 抬升

### 输入

- placeholder 固定「聊点什么...」（不随生成态切换）

### 历史跳转

- **入口**：搜索栏 accessory 内历史按钮（`Actions.vue`，`i-ri-chat-history-line`，置于新会话钮左侧）
- **浮层**：点击弹 `dropdown-panel` 浮层（`useFloating` `bottom-end` + Teleport body），列出本会话所有 user 消息（折叠空白 + 截断 60 字 + 空消息回退序号）
- **跳转**：点击列表项 → 关浮层 → `document.querySelector('[data-msg-id]')` `scrollIntoView({ block: 'center' })`（View.vue 的 user 行带 `data-msg-id`）
- **滚动联动**：跳转触发滚动 → View.vue `onScroll` 自然更新 `stickToBottom=false`，无需跨组件协调
- **禁用态**：有 user 消息前 disabled
- **注销**：模块切换 accessory unmount 时监听器随 `onUnmounted` 注销

## 文件结构

```
extensions/agent/
├── index.ts               # 注册
├── config.ts              # defineConfig + 选用 helpers
├── agent.ts               # useAgentChat composable（前端状态机）
├── view-logic.ts          # View 纯函数（streamView / showToolBody / buildHistoryLabel 等）
├── logic.ts               # 纯逻辑（消息序列化等，便于测试）
├── View.vue               # 布局 + 贴底滚动 + 悬浮操作 + notice
├── AgentTextPart.vue      # 流式/完成 markdown 文本 part
├── AgentReasoningPart.vue # 思考模式 part（三行省略，不回灌 LLM）
├── AgentToolStep.vue      # 工具步骤行 + output
├── agent-step.css         # 思考/工具步骤共用样式
├── Settings.vue           # Provider + Agent 配置
├── Actions.vue            # 模型切换 + 历史跳转 + 新会话 + 设置
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
