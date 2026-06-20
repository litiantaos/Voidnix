# Agent 扩展

AI 助手扩展，整合对话、网络搜索、命令执行。

## 三层架构

```
┌──────────────────────────────────────────────────┐
│  Agent Loop（engine/loop_runner.rs）              │
│  loop { call_llm → parse tool_calls → 审批 →     │
│         执行 → scrub_secret → 回灌 role:tool →    │
│         下一轮 }，max_turns 由 config 注入         │
├──────────────────────────────────────────────────┤
│  Tool Registry（engine/tool_registry.rs）         │
│  AgentTool trait: name/schema/requires_approval/  │
│                  call                             │
├──────────────────┬───────────────────────────────┤
│  web_search      │  run_command                   │
│  (Tavily API)    │  (纵深防御 9 层)              │
└──────────────────┴───────────────────────────────┘
```

Engine 代码在 `extensions/agent/native/engine/`（从框架层下沉，扩展自管）。

## 通信协议

`agent_run` 接收 `Channel<AgentEvent>`，立即返回 `session_id`，后台 spawn task 跑 loop。

事件流（`AgentEvent` 枚举）：

- `TextDelta { text }`：LLM 文本增量
- `ToolCallStart { id, name }`：工具调用开始
- `ToolCallArgs { id, args }`：完整参数（JSON）
- `ApprovalRequired { id, toolName, args }`：需用户审批
- `ToolResult { id, ok, output }`：工具结果（已净化）
- `Completed`：本轮结束
- `Error { message }`：错误终止

前端手写类型 `src/types/agent.ts` + 裸 `invoke()`。

## Agent 工具

### web_search

Tavily 搜索（专为 AI 设计，返回含 `answer` 字段的结构化 JSON）。

- API：`POST api.tavily.com/search`，Bearer auth
- 免费额度：1000 次/月，无需信用卡
- 未配 API Key 时工具返回错误，引导用户去设置

### run_command

纵深防御 9 层：

1. 命令白名单（用户在 config 编辑；默认仅只读集合 ls/cat/grep/rg/fd/wc/file/stat/diff/jq/bat 等；执行器/写操作工具 find/awk/sed/git/cp/mv/ln/tee/truncate/touch/mkdir 即便用户加入 trusted 也会被 `TRUSTED_DENYLIST` 强制剔除）
2. FORBIDDEN 硬禁（`osascript/sudo/sh/curl/wget/...`，不可被白名单覆盖）
3. 参数黑名单（`--exec`/`-exec`/`-execdir`/`-ok`/`-okdir`/`--upload-pack`/`-o`/`-C`/...）+ shell 元字符检测
4. 断路器（`rm -rf /` / `rm -rf ~` 即便 approved 也拦）
5. `env_clear()` + 白名单 env（防父进程 API key 进子进程）
6. cwd `canonicalize`
7. `pre_exec` 设 rlimit（CPU / 内存 / 文件描述符上限由 config 配置）
8. `tokio::time::timeout` + `kill_on_drop(true)`（超时由 config 配置）
9. 输出边读边截断 + `scrub_secret` gitleaks 打码（最后一道兜底）

**三档审批**：

- 白名单内 + 无危险参数 → 直接执行
- FORBIDDEN 硬禁 → 直接拒
- 未知命令 / 危险参数 → 弹 `BaseDialog`，按钮「执行 / 执行并信任 / 取消」
- 「执行并信任」→ 追加到 `config.trustedCommands`

### System Prompt

每次 agent_run 注入 system message（messages[0]），由两部分组成：

1. **默认 harness**（`agent/native/mod.rs::DEFAULT_SYSTEM_PROMPT`，扩展自管）：描述 agent 角色、工具使用规则、安全约束、输出风格
2. **用户自定义**（可选）：在 config 配置，追加为「用户自定义指令」段

## 配置

agent 配置通过 `defineConfig` 自管，持久化至 `extensions/agent/config.json`：

```typescript
// extensions/agent/config.ts
defineConfig('agent', {
  trustedCommands: ['ls', 'cat', 'pwd', 'echo', ...],
  systemPrompt: '',
  searchProviders: [{ id: '...', type: 'tavily', apiKey: '' }],
  activeSearchProviderId: '',
})
```

AI Provider 基础设施（endpoint/apiKey/models）在框架级 `stores/settings.ts`，translate + agent 共享。

## 文件结构

```
extensions/agent/
├── index.ts               # module 注册（id 'agent'）
├── config.ts              # defineConfig（trustedCommands/systemPrompt/searchProviders + BOUNDS UI 镜像）
├── agent.ts               # useAgentChat composable（前端状态机）
├── View.vue               # part 渲染 + Approval 弹窗
├── Settings.vue           # Provider + Agent 配置
├── Actions.vue            # 模型切换 + 新会话
└── native/
    ├── mod.rs             # agent_run / agent_approve / agent_abort + Extension impl
    ├── policy.rs          # floor/cap/TRUSTED_DENYLIST 权威源（FORBIDDEN_FLOOR 31 / DENIED_ARG_FLOOR 19）
    ├── engine/            # agent 引擎（从框架层下沉）
    │   ├── mod.rs         # AgentEvent 枚举
    │   ├── loop_runner.rs # 主循环（max_turns/default_prompt 由 LoopInput 注入）
    │   ├── approval.rs    # ApprovalManager（oneshot channel）
    │   ├── cancellation.rs # SessionRegistry（CancellationToken）
    │   ├── trim.rs        # 历史消息裁剪
    │   ├── secret_scrub.rs # gitleaks 正则打码
    │   └── tool_registry.rs # AgentTool trait + ToolRegistry
    └── tools/
        ├── web_search.rs
        └── run_command.rs
```

## 测试

- Rust 单元测试：`cargo test --lib`（含 run_command 17 个防御测试 + approval 3 个 + policy 5 个 + LLM security 5 个）
- 前端测试：`bun run test`
