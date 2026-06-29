# translate

选中文本翻译 + 输入即译。全局快捷键抓取选中文本（Accessibility API / 剪贴板兜底），并行调用多引擎并排展示。目标语言自动反转（中文→英文，反之亦然）。

## 配置

`extensions/translate/config.ts`（defineConfig 自管）：

```typescript
defineConfig('extensions/translate/config', {
  targetLang: 'zh',
  configs: [{ id, type: 'youdao', appKey: '', appSecret: '' }] as TranslateApiConfig[],
})
```

`TranslateApiConfig` 为判别联合（`YoudaoConfig | AiConfig`），`type` 是创建时确定的不可变 discriminator，不同引擎只保存各自所需字段（youdao: appKey/appSecret；ai: endpoint/apiKey/models/prompt），避免互补字段空值平铺。

configs 为多引擎并发集合（`translateText` 遍历每项独立翻译并排展示），无「激活」概念。CRUD helpers：`addTranslateConfig()`（默认新增 ai 型）/ `updateTranslateConfig(id, partial)`（partial 不含 id/type）/ `removeTranslateConfig(id)`（保底保留 1 项）。

AI 型引擎的后端实现复用框架级 `runtime::llm` 基础设施（与 agent 扩展共享 `stream_openai_request` 管道）；provider 配置由 translate 自管（`configs: TranslateApiConfig[]`），与 agent 的 `aiProviders` 各自独立、互不复用。

## 后端

- `native/mod.rs`：Extension trait + shortcut hook 注册 + `get_selected_text_cached` 命令（SELECTED_TEXT 自管，不泄漏框架）
- `native/ai_translate.rs`：AI 流式翻译（复用 `runtime::llm` 基础设施）
- `native/youdao.rs`：有道翻译（SHA256 签名）
- `native/lang_utils.rs`：中英检测 + smart_target_lang

划词取词流程：快捷键触发 → `platform::selection::try_ax()`（AX 直取）→ 失败则 `platform::input::post_combo("cmd+c", Some(pid))` → `platform::selection::poll_clipboard(snap)`（轮询 + `platform::pasteboard` 快照恢复）。
