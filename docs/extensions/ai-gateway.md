# AI 网关

统一本地 AI 网关：进程内 **Anthropic Messages / OpenAI Responses / OpenAI Chat Completions 三协议直通反向代理**，按请求模型名路由到提供商 + 多 Key 轮换。任何说这三种协议的工具把 base_url 指向 `127.0.0.1:8788` 即可接入（Key 填占位值，网关按路由注入真实 Key）；Claude Code 因无法自配多提供商，提供**接管开关**（默认关，与网关开关联动——开接管自动启用网关、关网关连带还原）改写/还原其 settings.json。

## 链路

```
Claude Code ──POST /v1/messages──▶ 127.0.0.1:8788 ──按 body.model 路由──▶ 提供商 anthropicEndpoint
Responses 客户端 ──POST /v1/responses──▶ 同端口 ──▶ 提供商 responsesEndpoint
Chat 客户端 ──POST /v1/chat/completions──▶ 同端口 ──▶ 提供商 chatEndpoint（OpenAI Chat URL）
```

- **路由**：请求体 `model` 字段 → 提供商；`[Nm]` 长上下文后缀双向归一（CC 发送前已剥，中枢可能带后缀存储）
- **轮换**：每提供商多 Key，401-408/429/5xx 换下一把重发，lastGood 粘性优先；失败 Key 冷却排队尾（429 对齐上游 `Retry-After`，默认 60s；**配额耗尽型**——错误体含计费/配额/余额/周期限额文案（quota/billing/balance/额度/欠费/monthly 等）——冷却提至 30min，60s 对「今日额度已用完」这类窗口型耗尽是必然失败的空转）；**最后一 Key 的瞬时失败原地重打**至多 2 轮（429/5xx 且非配额型；1s 倍增，Retry-After ≤ 8s 时优先采用，超 8s 直接回放）——网关内吸收瞬时压力，客户端无感且不消耗其重试预算；全部失败回放最后一个上游错误（429 等），客户端可见真实原因。**已治理上游让位**：配额文案判定跳过（末跳 429 的 Retry-After 是权威，文本分类对网关错误体可能误判），冷却恒 `max(Retry-After, 60s)` 且原地重试不被配额分支禁掉
- **会话亲和**：`messages[0]`（Anthropic/Chat 面）/ `input[0]`（Responses 面）hash 为会话指纹（会话 append-only，首条全程不变），同一会话粘住上次应答的 Key——上游 prompt cache 按 Key 隔离，换 Key = 前缀 cache 作废全价重算；亲和优先级高于 lastGood，保持 24h（对齐 cache 冷却 5min × 长会话生命周期），过期写入时惰性清理。**提交时机在流完整结束**（流守护，见下）——收到 2xx 响应头 ≠ 应答成功，流中途断开时该 Key 上 prompt cache 并未建立，粘坏 Key = 客户端重试再撞同款失败，故不提交（粘性同理），重试自然换路；4xx 结构化错误即时提交（错误体小且完整到达，证明 Key 存活）
- **GLM effort 翻译**：GLM 5.2/5.3 的思考强度不认 `thinking.budget_tokens`（实测无控制力）、走 `output_config.effort`（实测生效）——Anthropic 面带 thinking 预算的 GLM 请求自动翻译（budget ≥10k → high / ≥4k → medium / 其余 low），CC 的 `EFFORT_LEVEL` 由此真正生效
- **透传**：请求体整体缓冲（换 Key 重放需完整 body，上限 128MB），响应 SSE 字节流直 pipe 不落盘；剥 host/鉴权/逐跳头后注入 `x-api-key` + `Bearer`；上游走 `http::stream_client()`（建连 30s、读间隙 120s，无整体超时，SSE ping 保活）。**唯一请求体归一**（`normalize_body`，thinking 处理参考 magpie `thinkingOffUnlessAsked`/`fitAutoModeClassifier`）：① 剥 model 的 `[Nm]` 客户端后缀（全协议面）；② Anthropic 面对未提 thinking 且非 claude 原生模型的请求注入 `thinking: {"type": "disabled"}`——DeepSeek 认此参数彻底关思考，智谱忽略它；③ GLM 5.2/5.3 的 thinking budget 翻译为 `output_config.effort`（智谱不认 budget，认 effort）；④ **CC auto mode 分类器特判**，双判据取并集：形态为主（无工具定义 + 显式 thinking disabled + `max_tokens` ≤ 128——跨 CC 版本稳定，不依赖 prompt 字面标签）、标签为辅（`<transcript>` + `<block>`/`<severity>`，magpie automode 判据）；命中即 max_tokens 扩容 +2048 且不受「显式 thinking 豁免」约束（分类器自带 disabled 被智谱无视，64 预算被思考耗尽产出空答案即 CC 报「模型不可用」），GLM 压 effort 最低档；⑤ 未提 thinking 且 `max_tokens` < 256 的请求提升预算至 256。非轮换错误缓冲错误体后回传客户端并落 `errpass` 日志。**已治理上游让位**：归一整体跳过，转发体字节级原样（`[Nm]` 后缀留体——`find_route` 双向 `norm_model` 不影响路由匹配，末跳自剥幂等）
- **流守护**（`GuardStream`，全部 2xx 应答流统一过路）：粘性/亲和的提交点从「收到响应头」移到「流完整结束」；Anthropic 面 SSE 哨兵判异常（零内容/缺 stop_reason/缺 message_stop）时**向客户端注入协议原生 error 事件收尾**——流以错误而非静默截断结束，CC 感知失败可重试（「绝不伪造正常终止」不变量，参考 magpie streamFailure）；哨兵在场时上游传输错误一律吞掉转干净 EOF（内容已完整则无害收尾——message_stop 已到的连接尾断只是噪声；内容残缺则由注入事件承载失败语义）；无哨兵面（Responses/Chat/非流式 JSON）仅以「干净 EOF = 完整应答」判定，传输截断不提交不注入（客户端由断连感知）。**注入的覆盖边界**：对「流中途断」（message_stop 未到达）形态有效——注入的错误事件是客户端读到的最后一个事件；「零内容但完整终止」形态（空回复 + message_stop 已过路）下 message_stop 先于注入到达，客户端流读取到 message_stop 即止、注入事件不被消费，此形态仅 tail 日志有记录（客户端表现为空回复，CC 自行报错）——彻底覆盖需尾部扣流（扣住 message_stop 待内容确认再放行），不为该边界形态引入
- **上下文超长翻译**（`too_long_rewrite`，errpass 路径）：非轮换 4xx 错误体匹配厂商超长文案（`context_length_exceeded`/`maximum context length`/中文「上下文」等保守词表），命中即改写为协议标准错误（Anthropic 面 `"prompt is too long"` / OpenAI 族 `code=context_length_exceeded`，原文摘要保留在 message 供排障）——原文案（尤其中文厂商）CC 识别不了直接报错停会话，标准语义触发 CC 自动 compact 续命；周期限额文案（monthly/weekly token limit）刻意不误译（属配额语义，误译会触发无意义 compact 丢上下文）。**已治理上游让位**：末跳已译为协议标准错误，本跳原样透传（词表重复命中是噪声）
- **不做跨协议翻译**：三个面各自直通对应端点，零语义损耗；智谱（`https://open.bigmodel.cn/api/anthropic`）、DeepSeek（`https://api.deepseek.com/anthropic`）均有官方 Anthropic 兼容端点

## 端口与生命周期

- 模块分工（`native/`）：`server`（Protocol 词汇 + 路由表 + Gateway 状态机 + 轮换策略 + HTTP 骨架与 proxy 主流程 + 响应构造）、`normalize`（请求体读取与归一纯函数：model 提取/会话指纹/模型名归一/normalize_body）、`guard`（2xx 流守护 + 尾部哨兵 + 字节扫描基元）、`cc_settings`（CC 接线）

- release **8788**（固定端口，CC 配置一次写入不再变）；dev **8789**（与 release 常驻并存不互抢；`logic.ts::GATEWAY_PORT` 与 Rust `server.rs::PORT` 双端手动同步）
- 服务器跑在 app tokio runtime 内（`axum` 最小特性集 http1 + tokio）；app 常驻 Accessory + monitor LaunchAgent 守护
- 启动链：扩展 Rust `setup` 读持久化快照直接拉起（前端就绪前的冷启动窗口 CC 无感）；前端配置就绪后经 `ai_gateway_sync` 全量刷新
- 快照 `extensions/ai-gateway/gateway-state.json`（enabled + 路由表，0600 原子写，含 Key 明文）
- 排障日志 `extensions/ai-gateway/gateway.log`：只记异常路径（route 路由失败，行含 method+path / upstream 上游状态码与错误摘要（配额型带 `quota` 标记）/ net 网络错误 / retry 原地重试 / replay 回放 / exhaust Key 耗尽 / tail 尾部哨兵 / toolong 超长翻译），epoch 毫秒时间戳（`date -r 秒` 转可读），超 512KB 整文件重置，新建即 0600；成功请求零记录、不含 Key 与请求体。**tail 尾部哨兵**：Anthropic 面 2xx SSE 流保持零解析直 pipe，字节过路做子串探测（text/tool/thinking 内容证据 + stop_reason + message_stop），流终止时判异常——零内容（无 text 无 tool）或未收 stop_reason 或未收 message_stop——记一行 `tail model · route · text=N tool=N think=Y stop=max_tokens end=ok|trunc` 并**向客户端注入协议原生 error 事件收尾**（见流守护）——「200 但语义可疑」从纯日志观测升级为客户端可感知失败（GLM 无视 thinking disabled 耗尽预算的空响应、裸 JSON 错误行致流静默终止等，此前零痕迹，CC 只报「模型不可用」且拿到静默截断流）；跨 chunk 边界的标记子串经残余拼接防漏检，stop_reason 值经末尾 4KB 环形缓冲提取；正常完成的流零记录

## 路由表来源

中枢（ai-providers）→ `buildRoutes`：三种端点（`anthropicEndpoint` / `responsesEndpoint` / `chatEndpoint`）至少声明一个 + 至少一把非空 Key + 至少一个模型的提供商才参与；`governed`（已治理上游）标记随路由透传到 Rust。hub deep watch（400ms 防抖）→ `ai_gateway_sync` 推 Rust,**改 Key / 加模型即时生效，工具无需重启**；路由表未变时（改开关/别名等无关 sync）保留冷却/亲和/粘性运行时状态——亲和清空 = 活跃会话丢 Key 粘性，上游 prompt cache 按 Key 隔离即全价重算；切换 governed 属换表（行为变化），运行时状态一并清（一次性亲和代价）。请求体归一在路由命中后才执行，未知模型不支付归一成本。

## 级联上游（governed）

provider 标记「已治理上游」（ai-providers 表单开关）= 端点本身是另一 AI 网关（级联末跳，如团队网关）。核心原则**知识在哪一跳，智能就在哪一跳**：归一/错误翻译/配额判定依赖「真实上游模型」（经末跳别名层解析后才知道）与「key 池知识」，只有末跳有——第一跳遇到网关上游时降级为哑管道。三让位（见链路节对应条目）：`normalize_body` 整体跳过、`too_long_rewrite` 跳过、配额文案判定跳过；全部保留：多 Key 轮换、会话亲和、最后一 Key 原地重试、流守护/尾部哨兵（第一跳↔末跳间传输断裂仍需本跳防御；双方哨兵各注入一个 error 事件时先到生效无害）。

接线契约：模型名用末跳矩阵**真名**（避开其别名层改写，归一判定才不错位）；末跳 429 的 `Retry-After` 权威（月度配额精确到月初，本跳直接对齐）；末跳响应头原样透传（`x-gateway` 识别头全程可见，可用于探测）；注意尺寸断层（本端收 128MB，末跳网关常见上限 32MB，超长上下文在第二跳 413）与重试叠乘（本端原地 ≤2 轮 × 末跳轮换 + 原地，最坏单请求分钟级）；末跳侧建议专用 virtual key（独立配额/限流，管理面可识别级联消费者）。

## Claude Code 接线

接管由 `ccTakeover` 开关控制（默认关——不擅改用户配置文件；UI 与网关开关联动：开接管自动启用网关、关网关连带还原接管，维持 `ccTakeover ⇒ enabled` 不变式——接管依赖网关运行，不允许「接管开着而网关关着」的虚假态，sync 仍双条件判定不依赖该不变式），实际接管需网关实际运行中（bind 成功——绑定失败时写入会把 CC 指向死端口或占用端口的陌生进程）且存在可接线载荷；任一条件失守（关接管/关网关/绑定失败/删光 Anthropic 端点）即还原，sync 调用本身失败（状态未知）时本轮不动接线。**跨实例互斥**：dev/release 共管同一 `~/.claude/settings.json`，接管是排他资源——`apply_at` 内（单次读文件零 TOCTOU）判当前 base_url 指向另一构建的网关端口（锚定 `http://127.0.0.1:` + 端口对，远程主机同端口不误判）且 `/health` 探活验明是对侧网关（无关进程恰好占端口不算活）即拒绝并经接管失败红字提示；探测失败 = 上次异常退出的死残留，放行覆盖自愈（此时原始态不可考，快照记全摘除——还原即摘除全部自有键，接管标记仍成立、还原不变式不失效）。`cc_remove` 同做归属校验：base_url 指向另一构建端口时只清本实例快照不覆写文件，防陈旧快照拆掉对侧的活动接管。接管期间读-合-写 `~/.claude/settings.json` **自有键**（serde_json `preserve_order` 保用户键序）：

- `env.ANTHROPIC_BASE_URL` = `http://127.0.0.1:{port}`；`env.ANTHROPIC_AUTH_TOKEN` = 占位（网关注入真实 Key，摘除 `apiKeyHelper`，不再依赖 shell env 链路）
- `env.ANTHROPIC_DEFAULT_{SONNET,OPUS,HAIKU}_MODEL` = 三档模型映射（新会话开局默认 / 经 `/model` 手动切换的旗舰档 / 后台小任务——标题生成等小流量，不钉住则走主模型烧额度）；空值摘除该键
- `env.CLAUDE_CODE_AUTO_MODE_SERVER` = `"0"`：关闭 auto mode 服务端分类器检查——CC v2.1.278+ 走网关时默认请求服务端检查（v2.1.282+ 不拉 feature flags 的会话亦然，与 `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` 叠加时每会话都问），而该检查的应答（响应流 `safeguard_results`）只存在于 Anthropic 官方端点，上游是第三方时永不 eligible，fallback 成立即弹「计费变更不适用本会话」提示（Enter 仅抑制 24h）。设 0 让 CC 不再请求服务端检查，分类器请求照旧自发走网关由归一特判兜底，兼防「响应无 verdict 致动作被拒」暗坑；还原时摘除该键（CC 回默认行为，直连官方保留免费服务端检查机会）
- `env.ANTHROPIC_DEFAULT_*_MODEL` 与 `modelPicker` 的模型 id 由 `ccContext` 档位（`'default' | '1m'`，UI 下拉直存，档值即 `[<tier>]` 后缀内容，默认 1m）统一决定形态：档值 = 追加 `[<tier>]` 后缀（CC 私有语法，识别后发对应长上下文 beta 头；已带后缀防双写），default = 统一剥成裸名——该项是 CC 侧形态的唯一决定因素，中枢存储带不带后缀都无影响；picker 的 label 恒裸名。新增档位（如 2m）仅扩类型联合 + UI 选项各一行，Rust `norm_model` 已按 `[Nm]` 泛化
- `modelPicker`（CC v2.1.242+，旧版本忽略未知键）= 全部 Anthropic 可路由模型 + `replaceBuiltInOptions`——**CC 内 `/model` 直接切换任何模型**，新会话生效
- 备份：首次触碰前写 `settings.json.voidnix-bak` 原文 + `cc-backup.json` 自有键精确快照（扩展数据目录）；关闭接管按快照逐键还原，用户自有键（`CLAUDE_CODE_EFFORT_LEVEL` 等）始终不碰

## 界面

设置列表两组：网关（总开关，副标题即运行态、绑定失败整行标红 + 尾随「AI 提供商」导航行——整行可点跳转提供商扩展，路由表来源即在那边管理）、Claude Code（接管开关组首，与网关开关联动——开接管自动启用网关、关网关连带还原，接管失败红字暴露原因；随后上下文档位下拉（默认/1M）与 Sonnet/Opus/Haiku 三档模型下拉（档位用途在小字说明：新会话开局默认 / 经 /model 手动切换的旗舰档 / 后台小任务），仅接管开启时展示，不限制 `/model` 范围）。使用说明（任何工具的接入方式、CC 自动接线、热更新等）经搜索栏 info 按钮的 markdown 弹窗承载（`Actions.vue`，与 ai-providers 帮助弹窗同款）。无可路由提供商时空态引导去 AI 提供商声明端点。

## 命令

- `ai_gateway_sync`（enabled + 路由表 → 启停与状态）/ `ai_gateway_status`
- `ai_gateway_cc_apply` / `ai_gateway_cc_remove`（接管 / 还原）
