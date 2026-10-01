# AI 网关

统一本地 AI 网关：进程内 **Anthropic Messages / OpenAI Responses / OpenAI Chat Completions 三协议直通反向代理**，按请求模型名路由到提供商 + 多 Key 轮换。任何说这三种协议的工具把 base_url 指向 `127.0.0.1:8788` 即可接入（Key 填占位值，网关按路由注入真实 Key）；Claude Code 因无法自配多提供商，提供**接管开关**（独立于网关开关，默认关）联动改写/还原其 settings.json。

## 链路

```
Claude Code ──POST /v1/messages──▶ 127.0.0.1:8788 ──按 body.model 路由──▶ 提供商 anthropicEndpoint
Responses 客户端 ──POST /v1/responses──▶ 同端口 ──▶ 提供商 responsesEndpoint
Chat 客户端 ──POST /v1/chat/completions──▶ 同端口 ──▶ 提供商 endpoint（API URL）
```

- **路由**：请求体 `model` 字段 → 提供商；`[1m]` 后缀双向归一（CC 发送前已剥，中枢可能带后缀存储）
- **轮换**：每提供商多 Key，401-408/429/5xx 换下一把重发，lastGood 粘性优先；失败 Key 冷却 60s 排队尾（429 对齐上游 `Retry-After`），全部失败回放最后一个上游错误（429 等），客户端可见真实原因
- **会话亲和**：`messages[0]` hash 为会话指纹（会话 append-only，首条全程不变），同一会话粘住上次应答的 Key——上游 prompt cache 按 Key 隔离，换 Key = 前缀 cache 作废全价重算；亲和优先级高于 lastGood，保持 24h（对齐 cache 冷却 5min × 长会话生命周期），过期写入时惰性清理
- **GLM effort 翻译**：GLM 5.2/5.3 的思考强度不认 `thinking.budget_tokens`（实测无控制力）、走 `output_config.effort`（实测生效）——Anthropic 面带 thinking 预算的 GLM 请求自动翻译（budget ≥10k → high / ≥4k → medium / 其余 low），CC 的 `EFFORT_LEVEL` 由此真正生效
- **透传**：请求体整体缓冲（换 Key 重放需完整 body，上限 128MB），响应 SSE 字节流直 pipe 不落盘；剥 host/鉴权/逐跳头后注入 `x-api-key` + `Bearer`；上游走 `http::stream_client()`（建连 30s、读间隙 120s，无整体超时，SSE ping 保活）。**唯一请求体归一**（`normalize_body`，参考 magpie 的 `thinkingOffUnlessAsked`）：① 剥 model 的 `[1m]` 客户端后缀（全协议面——CC 主对话自剥 + 发 beta 头，但其分类器等旁路请求原样带后缀，上游不认识该语法必报「模型不存在」）；② Anthropic 面对未提 thinking 且非 claude 原生模型的请求注入 `thinking: {"type": "disabled"}`——原生 Anthropic 语义即「无该字段 = 不思考」，兼容端点默认思考，CC 分类器/标题等旁路小请求的输出预算被思考耗尽产出空 text；DeepSeek 认此参数彻底关思考，智谱忽略它（由③兜底）；③ 对未提 thinking 且 `max_tokens` 低于 256 的请求提升预算至 256（智谱忽略 disabled 依然思考且计入 max_tokens，下限保证 text 有出口；模型答完即停，无额外消耗）。非轮换错误（400 等请求级错误）直接透传客户端但落 `errpass` 日志
- **不做跨协议翻译**：三个面各自直通对应端点，零语义损耗；智谱（`https://open.bigmodel.cn/api/anthropic`）、DeepSeek（`https://api.deepseek.com/anthropic`）均有官方 Anthropic 兼容端点

## 端口与生命周期

- release **8788**（固定端口，CC 配置一次写入不再变）；dev **8789**（与 release 常驻并存不互抢；`logic.ts::GATEWAY_PORT` 与 Rust `server.rs::PORT` 双端手动同步）
- 服务器跑在 app tokio runtime 内（`axum` 最小特性集 http1 + tokio）；app 常驻 Accessory + monitor LaunchAgent 守护
- 启动链：扩展 Rust `setup` 读持久化快照直接拉起（前端就绪前的冷启动窗口 CC 无感）；前端配置就绪后经 `ai_gateway_sync` 全量刷新
- 快照 `extensions/ai-gateway/gateway-state.json`（enabled + 路由表，0600 原子写，含 Key 明文）
- 排障日志 `extensions/ai-gateway/gateway.log`：只记异常路径（route 未知模型 / upstream 上游状态码与错误摘要 / net 网络错误 / replay 回放 / exhaust Key 耗尽），epoch 毫秒时间戳（`date -r 秒` 转可读），超 512KB 整文件重置；成功请求零记录、不含 Key 与请求体

## 路由表来源

中枢（ai-providers）→ `buildRoutes`：三种端点（`anthropicEndpoint` / `responsesEndpoint` / `endpoint`，后者即 chat completions 的 API URL）至少声明一个 + 至少一把非空 Key + 至少一个模型的提供商才参与。hub deep watch（400ms 防抖）→ `ai_gateway_sync` 推 Rust,**改 Key / 加模型即时生效，工具无需重启**。

## Claude Code 接线

接管由独立开关 `ccTakeover` 控制（默认关——网关开关只管起停服务，不擅改用户配置文件），实际接管需 `enabled && ccTakeover` 且存在可接线载荷；任一条件失守（关接管/关网关/删光 Anthropic 端点）即还原。接管期间读-合-写 `~/.claude/settings.json` **自有键**（serde_json `preserve_order` 保用户键序）：

- `env.ANTHROPIC_BASE_URL` = `http://127.0.0.1:{port}`；`env.ANTHROPIC_AUTH_TOKEN` = 占位（网关注入真实 Key，摘除 `apiKeyHelper`，不再依赖 shell env 链路）
- `env.ANTHROPIC_DEFAULT_{SONNET,HAIKU}_MODEL` = 新会话默认模型 + 后台任务模型（标题生成等小流量；不钉住则走主模型烧额度）。opus 档不写且摘除历史残留——`modelPicker` 替换内置阵容后别名不可达，属死配置
- `env.ANTHROPIC_DEFAULT_*_MODEL` 与 `modelPicker` 的模型 id 由 `cc1mContext` 开关统一决定形态（默认开）：开 = 追加 `[1m]` 后缀（CC 私有语法，识别后发 context-1m beta 头启用百万 token 上下文；已带后缀防双写），关 = 统一剥成裸名——开关是 CC 侧形态的唯一决定因素，中枢存储带不带后缀都无影响；picker 的 label 恒裸名
- `modelPicker`（CC v2.1.242+，旧版本忽略未知键）= 全部 Anthropic 可路由模型 + `replaceBuiltInOptions`——**CC 内 `/model` 直接切换任何模型**，新会话生效
- 备份：首次触碰前写 `settings.json.voidnix-bak` 原文 + `cc-backup.json` 自有键精确快照（扩展数据目录）；关闭接管按快照逐键还原，用户自有键（`CLAUDE_CODE_EFFORT_LEVEL` 等）始终不碰

## 界面

设置列表三组：网关（单一总开关，副标题即运行态、绑定失败整行标红）、Claude Code（接管开关组首 + 1M 上下文开关与默认模型两档下拉，别名细节仅接管开启时展示，接管失败红字暴露原因，CC 接线细节，低频：不限制 `/model` 范围）、可路由模型（全工具状态展示：模型 → 提供商 · 协议 · Key 数）。使用说明（任何工具的接入方式、CC 自动接线、热更新等）经搜索栏 info 按钮的 markdown 弹窗承载（`Actions.vue`，与 ai-providers 帮助弹窗同款）。无可路由提供商时空态引导去 AI 提供商声明端点。

## 命令

- `ai_gateway_sync`（enabled + 路由表 → 启停与状态）/ `ai_gateway_status`
- `ai_gateway_cc_apply` / `ai_gateway_cc_remove`（接管 / 还原）
