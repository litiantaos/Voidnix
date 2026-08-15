# AI 提供商

统一维护 OpenAI 兼容 **URL / 多 Key / 模型**。只做配置中枢，**不维护「使用中」**；谁用哪套由消费者自选。

## 职责边界

- **本扩展**：CRUD 提供商与 Key、粘贴出去、智谱额度展示、写 `ai.env` + shell 钩子
- **消费者**：自行持久化选用（如 Agent 的 `providerModelKey`，翻译 AI 的 `selections`）
- **解析**：`resolveCredentials({ providerId, keyId?, model? })` 按传入选用取值（缺项可由 env 补全），中枢不猜默认提供商；Agent 无显式选用时自行默认首个可用提供商

## schema 变更

- **normalizeProvider**：中枢加载后对 `keys[]` 做 `normalizeProvider`（旧单 `apiKey` → keys）
- **空中枢导入**：中枢为空时一次性从旧 `extensions/agent/config.json` 的 `aiProviders` / translate 旧 AI 引擎字段导入，并尽量删掉旧密钥字段；消费者侧也会清悬空选用
- **重建**：仍可直接删磁盘 config 按 defaults 重建

## 界面（Key 为一等公民）

列表按**提供商分组**（分组名 = **名称**，空则 URL 推导域名如 `OPENAI`），**每把 Key 单独一行**：

- **行**（标准列表项）：
  - 标题 = 备注
  - 副标题 = `sk-… · MAX · 5h 12% (2.3h) · 7d 34% (2.3d) · 30d 1.2B`（重置缺失为 `—`）
  - 右侧 = **30d 曲线**（智谱）
- **回车**：打开编辑 Key 弹窗
- **Cmd+Enter / 右键**：统一「粘贴 Key / 粘贴 URL / 粘贴 {模型}」、删除 Key（经 `useActionPanel` 统一 `toggleOpen`，二次触发关闭）
- **分组标题右侧**：编辑提供商 · 添加 Key
- **添加提供商**：搜索栏右侧 `+`（`searchBarAccessory`）

弹窗：添加/编辑提供商（名称 / API URL / 模型；创建时含首把 Key）；添加/编辑 Key。无「选用 / 使用中」。

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

按 endpoint 自动识别（或 `usageKind` 显式指定）：

- **智谱 Coding Plan**（`bigmodel.cn` / `zhipuai`）：副标题与右侧 30d 曲线格式见「界面」（曲线对齐 [tokens-monitor](https://github.com/litiantaos/tokens-monitor)）。命令 `ai_providers_zhipu_quota`。
- **DeepSeek**（`deepseek.com`）：账户余额 `GET {origin}/user/balance`（Bearer Key）。列表副标题展示 `¥/ $` 总余额；无 5h/7d 窗口、无 30d 曲线。命令 `ai_providers_deepseek_balance`。

## CLI / env

### 写入规则

- **文件路径**：保存后写 `~/.config/voidnix[/dev]/ai.env`
- **release**：基础目录；shell 全局投影注入（`shell_rc` 幂等写入 `# voidnix ai-providers` source 块，见 [shell-rc.md](../shell-rc.md)）
- **debug**：叠 `.dev`，与 bundle id 隔离一致；只写 `voidnix.dev/ai.env` 文件，**不注入 shell**
- **dev/prod 不并存原因**：外部工具按私有名（`VOIDNIX_*`）显式引用，无法 dev/prod 并存，全局只放 prod
- **dev 凭证用途**：供 App 内回退与手动 `source ~/.config/voidnix.dev/ai.env` 验证

### 变量命名

全量 `VOIDNIX_` 私有前缀——不抢占外部工具约定的通用变量名（如 `ZHIPU_API_KEY`），外部工具须显式引用。`envKey` 显式可覆盖（逃生舱，不加前缀）。

- **知名端点**固定后缀：
  - 智谱 Coding Plan（`bigmodel.cn` / `zhipuai`）→ `VOIDNIX_ZHIPU_API_KEY` + `VOIDNIX_ZHIPU_BASE_URL`
  - DeepSeek（`deepseek.com`）→ `VOIDNIX_DEEPSEEK_API_KEY` + `VOIDNIX_DEEPSEEK_BASE_URL`
  - 其余按名称 / hostname 推导（如 OpenAI 端点 → `VOIDNIX_OPENAI_API_KEY`）
- **多 Key 命名**：第一把非空写规范名（`VOIDNIX_DEEPSEEK_API_KEY` 等）；其余按备注 ASCII 后缀（`VOIDNIX_DEEPSEEK_BACKUP_API_KEY`），纯中文备注回退 `VOIDNIX_DEEPSEEK_KEY2_API_KEY`，碰撞递增，不静默丢 Key
- **单 Key 规范名冲突**（两套同端点提供商）：第二套序号兜底（`VOIDNIX_DEEPSEEK_KEY1_API_KEY`），不静默丢
- **`VOIDNIX_*_BASE_URL`**：按**提供商**输出（endpoint 是提供商级属性），每提供商仅一条，不随 Key 重复

### 外部工具

中枢只写 env（`VOIDNIX_*` 私有名，key + url）；各工具自管模型选用（模型定义在工具配置里，含上下文长度/定价等元数据，不由中枢投射）。

- **OpenCode**：`opencode.json` 的 `provider.*.options.apiKey` 用 `{env:VOIDNIX_ZHIPU_API_KEY}` 等显式引用；baseURL 写在 `options.baseURL`。模型：`zhipuai-coding-plan/glm-5.2`、`deepseek/deepseek-v4-pro` 等
- **Grok Build**：`~/.grok/config.toml` 的 `[model.*]` 用 `env_key = "VOIDNIX_ZHIPU_API_KEY"` 等 + `base_url`；切模型 `/model glm-5-2-1m` 等

## 命令

- `ai_providers_export` / `export_dir` / `env_snapshot`
- `ai_providers_zhipu_quota` / `ai_providers_deepseek_balance`
- 框架 `pasteboard_paste_text`：隐藏主窗后注入 Cmd+V；**粘贴后密钥仍留在系统剪贴板**（与 clipboard 扩展粘贴路径一致，不自动清）

DeepSeek 余额请求对推导出的 URL 走 `http::validate_url` SSRF 门禁（首跳）。
