# AI 提供商

统一维护 OpenAI 兼容 **URL / 多 Key / 模型**。只做配置中枢，**不维护「使用中」**；谁用哪套由消费者自选。

## 职责边界

- **本扩展**：CRUD 提供商与 Key、粘贴出去、智谱额度展示、写 `ai.env` + shell 钩子
- **消费者**：自行持久化选用（如 Agent 的 `providerModelKey`，翻译 AI 的 `selections`）
- **解析**：`resolveCredentials({ providerId, keyId?, model? })` / `resolveRuntimeCredentials(sel)` 按传入选用取值，中枢不猜默认提供商

## schema 变更

中枢加载后对 `keys[]` 做 `normalizeProvider`（旧单 `apiKey` → keys）。  
中枢为空时一次性从旧 `extensions/agent/config.json` 的 `aiProviders` / translate 旧 AI 引擎字段导入，并尽量删掉旧密钥字段；消费者侧也会清悬空选用。  
仍可直接删磁盘 config 按 defaults 重建。

## 界面（Key 为一等公民）

列表按**提供商分组**（分组名 = **名称**，空则 URL 推导域名如 `OPENAI`），**每把 Key 单独一行**：

- 行：标准列表项——标题 = 备注；副标题 = `sk-… · MAX · 5h 12% / 2.3h · 7d 34% / 2.3d · 30d 1.2B tokens`（重置缺失为 `—`）；**右侧 = 30d 曲线**（智谱）
- 回车：打开编辑 Key 弹窗
- **Cmd+Enter**：统一「粘贴 Key / 粘贴 URL / 粘贴 {模型}」、删除 Key
- 分组标题右侧：编辑提供商 · 添加 Key
- **添加提供商**：搜索栏右侧 `+`（`searchBarAccessory`）

弹窗：添加/编辑提供商（名称 / API URL / 模型；创建时含首把 Key）；添加/编辑 Key。无「选用 / 使用中」。

## 多 Key

`keys: { id, label, apiKey }[]`。消费者解析时传 `keyId`；省略则取该提供商**第一把非空** Key。

选用串约定（Agent / 翻译 AI）：`providerId::keyId::model`（兼容旧式 `providerId::model`）。

删提供商或 Key 时中枢 `onAiProvidersChange` 通知消费者清悬空选用（agent `providerModelKey`、translate `selections`）。

## 额度 / 余额监控

按 endpoint 自动识别（或 `usageKind` 显式指定）：

- **智谱 Coding Plan**（`bigmodel.cn` / `zhipuai`）：副标题 `sk-… · MAX · 5h 12% / 2.3h · 7d 34% / 2.3d · 30d 1.2B tokens`（重置无则 `—`），右侧 30d 曲线（对齐 [tokens-monitor](https://github.com/litiantaos/tokens-monitor)）。命令 `ai_providers_zhipu_quota`。
- **DeepSeek**（`deepseek.com`）：账户余额 `GET {origin}/user/balance`（Bearer Key）。列表副标题展示 `¥/ $` 总余额；无 5h/7d 窗口、无 30d 曲线。命令 `ai_providers_deepseek_balance`。

## CLI / env

保存后写 `~/.config/voidnix/ai.env`（`shell_rc` 幂等注入，见 [shell-rc.md](../shell-rc.md)）。

- `OPENAI_*` = **列表中第一套完整** endpoint+key+model（方便只读 OPENAI 的工具，**不是全局 active**）
- 其余 Key 按备注导出命名变量

## 命令

- `ai_providers_export` / `export_dir` / `env_snapshot`
- `ai_providers_zhipu_quota` / `ai_providers_deepseek_balance`
- 框架 `pasteboard_paste_text`：隐藏主窗后注入 Cmd+V；**粘贴后密钥仍留在系统剪贴板**（与 clipboard 扩展粘贴路径一致，不自动清）

DeepSeek 余额请求对推导出的 URL 走 `http::validate_url` SSRF 门禁（首跳）。
