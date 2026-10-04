# AI 提供商

统一维护三种线协议端点 **OpenAI Chat（`chatEndpoint`）/ OpenAI Responses（可选）/ Anthropic Messages（可选）+ 多 Key / 模型**。只做配置中枢，**不维护「使用中」**；谁用哪套由消费者自选。

## 职责边界

- **本扩展**：CRUD 提供商与 Key、粘贴出去、智谱额度展示
- **消费者**：自行持久化选用（如 Agent 的 `providerModelKey`，翻译 AI 的 `selections`）
- **解析**：`resolveCredentials({ providerId, keyId?, model? })` 按传入选用取值，中枢不猜默认提供商；Agent 无显式选用时自行默认首个可用提供商

## schema 变更

- **normalizeProvider**：经 `defineConfig` 第三参 onLoad 在 backfill 后同步执行（`resolveReady` 前，全部 `whenConfigReady` 等待者读到迁移后形态）——旧字段名 `endpoint` → `chatEndpoint`、旧单 `apiKey` / 缺 `keys` → `keys[]`；新字段 `governed`（已治理上游）缺省兜 `false`
- **空中枢导入**：中枢为空时一次性从旧 `extensions/agent/config.json` 的 `aiProviders` / translate 旧 AI 引擎字段导入，并尽量删掉旧密钥字段；消费者侧也会清悬空选用
- **重建**：仍可直接删磁盘 config 按 defaults 重建

## 界面（Key 为一等公民）

列表按**提供商分组**（分组名 = **名称**，空则 URL 推导域名如 `OPENAI`），**每把 Key 单独一行**：

- **行**（标准列表项）：
  - 标题 = 备注
  - 副标题 = `sk-… · MAX · 5h 12% (2.3h) · 7d 34% (2.3d) · 30d 1.2B`（重置缺失为 `—`）
  - 右侧 = **30d 曲线**（智谱）
- **回车**：打开编辑 Key 弹窗
- **Cmd+Enter / 右键**：统一「粘贴 Key / 粘贴 OpenAI Chat URL / 粘贴 OpenAI Responses URL / 粘贴 Anthropic Messages URL（各自声明了对应端点才出现）/ 粘贴 {模型}」、删除 Key（经 `useActionPanel` 统一 `toggleOpen`，二次触发关闭）
- **分组标题右侧**：编辑提供商 · 添加 Key
- **添加提供商**：搜索栏右侧 `+`（`searchBarAccessory`）；列表空态（`BaseSetupState`，按钮文案覆盖为「添加提供商」）同款直达创建弹窗

弹窗：添加/编辑提供商（名称 / OpenAI Chat URL / 模型 / 可选 OpenAI Responses URL / 可选 Anthropic Messages URL / 已治理上游开关；创建时含首把 Key）；添加/编辑 Key。无「选用 / 使用中」。

## 多 Key

### 数据结构

`keys: { id, label, apiKey }[]`

### 解析规则

- 消费者解析时传 `keyId`
- 省略则取该提供商**第一把非空** Key

### 选用串约定（Agent / 翻译 AI）

- 格式：`providerId::keyId::model`（兼容旧式 `providerId::model`）
- 选用单位 = **Key × 模型**（非仅模型）

### 消费者 UI

- 工具：`modelSelectOptions` / `selectionDisplayLabel`
- **单 Key**：只显示模型名
- **多 Key**：显示 `模型 · 备注`（Agent 下拉触发器、翻译勾选主文案与设置摘要一致）
- 翻译弹窗在存在多 Key 时字段名改为「模型与 Key」

## 与消费者选用同步

选用由消费者自持；中枢变更后**不猜替代**。机制收敛为：

1. **唯一规则** `isCredentialSelectionValid`（提供商在 + 模型仍在 `models` + 有 keyId 时 Key 仍在）
2. **热路径读时过滤**（不写回）：
   - 翻译 `effectiveAiSelections`（校验 + 补全 keyId + 去重）/ `resolveAiTargets`
   - Agent `effectiveProviderModelKey` / resolve
3. **冷路径 prune**（写回干净）：双方 config ready 后一次
   - 翻译 `updateAiConfig` 写入时压滤
   - Agent `setProviderModelKey` 只接受有效串

### 翻译去重注意

- 旧式 `providerId::model`（无 keyId）与三段式同模型会算两条
- `canonicalizeAiSelection` 统一补 keyId 后按 `providerId::keyId::model` 去重，避免摘要/并发次数多于中枢可选项

### 架构边界

- 无 deep watch 中枢、无变更事件扇出
- 改名模型 = 删旧加新 → 读时视为未选，冷 prune 后落盘清空，需用户重选

## 额度 / 余额监控

按 chat 端点自动识别（或 `usageKind` 显式指定）。Rust 侧全部在 `native/usage/`（每提供商一文件 + 共享原语 `fetch_text` / `json_i64` / `json_f64`；获取协议差异大，不做配置驱动的统一抽象），新增提供商 = `usage/` 下新文件（`#[tauri::command]` 由 `sync:extensions` 自动注册）+ 前端 `resolveUsageKind` 分支与拉取函数：

**拉取时机**：用量是实时数据，每次进入扩展拉最新——KeepAlive 首挂载/重进均触发 activated；窗口唤起获焦（`window-focused`）补刷，回调自带 `activeExtId` 激活判断、失活期跳过；配置指纹变化（改 Key / 端点）同样重拉。拉取期间旧值原地保留静默替换，无缓存（首次进入）才显示加载态。

- **智谱 Coding Plan**（`bigmodel.cn` / `zhipuai`，`usage/zhipu.rs`）：副标题与右侧 30d 曲线格式见「界面」（曲线对齐 [tokens-monitor](https://github.com/litiantaos/tokens-monitor)）。命令 `ai_providers_zhipu_quota`，配额 `GET https://open.bigmodel.cn/api/monitor/usage/quota/limit`（Authorization = 裸 Key，最小请求头），与 30d 用量请求（`bigmodel.cn/api/monitor/usage/model-usage`，叠加浏览器请求面 UA/Referer/Origin 防网关拒）并发。HTTP 401 与信封 `code` 401/1001 均判无效 Key（401 判定先于 shape 分支）。响应兼容两种 shape：`{data:{limits,level}}` 信封（V2 `TOKENS_LIMIT`）与 V3（2026-07-30 积分制）顶层数组（`CREDIT_LIMIT`），均映射 5h（unit 3）/7d（unit 6）窗，非配额类型（如 `TIME_LIMIT`）跳过。`nextResetTime` 为毫秒。
- **DeepSeek**（`deepseek.com`，`usage/deepseek.rs`）：账户余额 `GET {origin}/user/balance`（Bearer Key）。列表副标题展示 `¥/ $` 总余额；无 5h/7d 窗口、无 30d 曲线。命令 `ai_providers_deepseek_balance`。

## 外部工具接入

外部工具不引用中枢凭证——统一经 [ai-gateway](ai-gateway.md) 接入（本地三协议网关按模型名路由 + Key 轮换 + 热更新）：API 地址指向 `http://127.0.0.1:8788`、Key 填占位值即可。各工具自管模型选用（模型定义在工具配置里，含上下文长度/定价等元数据，不由中枢投射）。

- **Responses 端点**（`responsesEndpoint`，可选）：语义是「Responses 端点与 chat 端点**不同**时的那个 URL」——分立端点（智谱 Responses `https://open.bigmodel.cn/api/v1` 与 chat `/api/coding/paas/v4`）才需要填；同端点用路径/参数区分协议的提供商（DeepSeek 等）留空即可。`chatEndpoint` 始终存 chat 端点（内部消费者 agent/translate 走 chat completions，不受影响）
- **Anthropic 端点**（`anthropicEndpoint`，可选）：Anthropic Messages 线协议端点（智谱 `https://open.bigmodel.cn/api/anthropic`、DeepSeek `https://api.deepseek.com/anthropic`），声明后模型进入网关的 Anthropic 路由（Claude Code 等客户端）
- **已治理上游**（`governed`，默认关）：端点本身是另一 AI 网关（级联末跳）时开启——仅 ai-gateway 消费，命中时网关的请求体归一/错误翻译/配额文案判定让位（知识只在末跳），详见 [ai-gateway](ai-gateway.md)「级联上游」

历史的 `ai.env` 导出（`VOIDNIX_*` 环境变量 + shell source 钩子）已移除：扩展 setup 对存量遗留（rc 注入块 / `~/.config/voidnix[/dev]/ai.env`）做幂等自清。

## 命令

- `ai_providers_zhipu_quota` / `ai_providers_deepseek_balance`
- 框架 `pasteboard_paste_text`：隐藏主窗后注入 Cmd+V；**粘贴后密钥仍留在系统剪贴板**（与 clipboard 扩展粘贴路径一致，不自动清）

DeepSeek 余额请求对推导出的 URL 走 `http::validate_url` SSRF 门禁（首跳）。
