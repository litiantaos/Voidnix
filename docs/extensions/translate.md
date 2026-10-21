# translate

选中文本翻译 + 输入即译。全局快捷键抓取选中文本（Accessibility API / 剪贴板兜底），并行调用多引擎并排展示。目标语言自动反转（中文→英文，反之亦然）。

## 配置

`extensions/translate/config.ts`（defineConfig 自管）：

```typescript
defineConfig('extensions/translate/config', {
  targetLang: 'zh',
  configs: [
    { id: 'service-youdao', type: 'youdao', appKey: '', appSecret: '' },
    { id: 'service-ai', type: 'ai', selections: [], prompt: '' },
  ],
})
```

固定两项服务（设置页不可增删）：

- **有道翻译**：appKey / appSecret
- **AI 翻译**：`selections: { providerId, keyId?, model }[]` 跨中枢多选 **Key×模型** + `prompt`；凭证只在 `@/runtime/ai-providers`

### 设置 UI

「翻译服务」下列出两项：

- AI 弹窗多选中枢选用：单 Key 仅显示模型名，多 Key 主文案为 `模型 · 备注`、字段名「模型与 Key」
- 可编辑提示词，并提供「管理提供商 / 打开 AI 提供商」跳转中枢扩展
- 无独立「模型」分组、无服务右侧加号

### AI 选用解析

- `resolveAiTargets` / UI 摘要走 `effectiveAiSelections`（读时按中枢过滤无效选用 + 补 keyId）
- `keyId` 缺省取第一把非空 Key；缺项可用 env 补
- 启动与 `updateAiConfig` 冷 prune 写回
- 运行结果引擎标签：单 Key 仅提供商名，多 Key 为「提供商 · 备注」

### 关思考（速度）

AI 翻译请求固定附带 `thinking: {"type": "disabled"}`。DeepSeek V4 等混合推理模型默认开思考（`deepseek-flash` 实测默认 effort=high），思考阶段不产出可见文本，表现为「窗口已弹出但迟迟不出字」：首字延迟增加约 0.2~1.7s、长文更明显（120 词段落 3 次中位数：首字 1.2s → 0.55s，总时长 1.8s → 1.2s）。翻译是确定性的语码转换，关思考后译文质量实测持平，同类浏览器翻译扩展（read-frog 等）也默认关思考。

机制落在 `runtime/llm` 的 `StreamConfig.extra_body`（提供商专有请求体字段，顶层浅合并）：服务端严格反序列化、以 400/422 拒绝未知字段时自动剥掉扩展字段重试一次（不认该参数的服务商如 OpenAI 原生不会因此挂掉），非流式 `openai_request_once` 同款。只有「剥掉后重试成功」才把该 endpoint/model 记入**进程级负缓存**，后续请求直接不再发送扩展字段——严格服务商的额外往返从「每次翻译一次」降为「每个进程生命周期一次探测」（重启或换模型/端点后重新探测）。agent 不传该字段（工具调用需要推理）。

**已知限制**：宽松忽略未知字段的服务商（多数 OpenAI-compatible 网关）不会报错，字段静默失效、仍会思考——这类要按模型名映射字段（read-frog 式查表），当前未做。接入 Qwen/Gemini/OpenAI 原生等模型前，先用一条短请求确认 `reasoning_content` / `reasoning_tokens` 归零，不要只看 HTTP 200。

### 未配置

View 空态（有道与 AI 均无可用凭证时）用统一 `BaseSetupState`（`components/ui/`），「去配置」进入本扩展设置子视图（`openSubview('config')`，有道 Key 与 AI 模型选用均在此，含跳转中枢入口）。

旧 AI 引擎字段启动时一次性导入中枢并 strip。详见 [ai-providers.md](./ai-providers.md)。

## 结果交互

- **回车 / 双击**：复制译文并隐藏窗口（`copyAndHide`）
- **朗读按钮**：每条译文右侧喇叭图标，点击用系统 TTS 朗读（框架 `speak_text` / `stop_speech` 命令，底层 `runtime/speech.rs` 的 say CLI 封装），再次点击停止。朗读中图标转 fill + 主色高亮，读完或切换结果自动复位；切走扩展 / 新一轮翻译自动停止。语种按译文字符脚本推断（`detectSpeechLang`：假名→ja、谚文→ko、汉字→zh、西里尔→ru、阿拉伯→ar、泰文→th、天城文→hi、越南文拉丁扩展→vi、其余→en），映射 `say -v` 系统预装语音

## Rust 端

- `native/mod.rs`：Extension trait + shortcut hook 注册 + `get_selected_text_cached` 命令（SELECTED_TEXT 自管，不泄漏框架）
- `native/ai_translate.rs`：AI 流式翻译（复用 `runtime::llm` 基础设施）
- `native/youdao.rs`：有道翻译（SHA256 签名）
- `native/lang_utils.rs`：中英检测 + smart_target_lang

划词取词流程：快捷键触发 → `platform::selection::try_ax()`（AX 直取）→ 失败则 `platform::input::post_combo("cmd+c", Some(pid))` → `platform::selection::poll_clipboard(snap)`（轮询 + `platform::pasteboard` 快照恢复）。

**文件场景排除**：Finder 选中文件时 AX 取不到文本，`cmd+c` 兜底会让 Finder 把文件写入剪贴板（含 `public.file-url`，顺带写入文件名纯文本）。取词链路在 `poll_clipboard` 与 `get_selected_text` 命令两处均经 `pasteboard::has_file_url()` 判定——剪贴板含文件 URL 即视为非文本选中，返回空、不进入翻译（避免文件名被当文本翻译）。
