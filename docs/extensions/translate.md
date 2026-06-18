# translate

选中文本翻译 + 输入即译。全局快捷键抓取选中文本（Accessibility API / 剪贴板兜底），并行调用多引擎并排展示。目标语言自动反转（中文→英文，反之亦然）。

## 配置

`extensions/translate/config.ts`（defineConfig 自管）：

```typescript
defineConfig('translate', {
  targetLang: 'zh',
  configs: [{ id, type: 'youdao', isDefault: true, ... }] as TranslateApiConfig[],
})
```

CRUD helpers：`addTranslateConfig()` / `updateTranslateConfig(id, partial)` / `removeTranslateConfig(id)`。

AI Provider 基础设施（endpoint/apiKey/models 共享）在框架级 `stores/settings.ts`。

## 后端

- `native/mod.rs`：Extension trait + shortcut hook 注册 + `get_selected_text_cached` 命令（SELECTED_TEXT 自管，不泄漏框架）
- `native/ai_translate.rs`：AI 流式翻译（复用 `runtime::llm` 基础设施）
- `native/youdao.rs`：有道翻译（SHA256 签名）
- `native/lang_utils.rs`：中英检测 + smart_target_lang

划词取词流程：快捷键触发 → `platform::selection::try_ax()`（AX 直取）→ 失败则 `platform::input::inject_copy(pid)` → `platform::selection::poll_clipboard(snap)`（轮询 + `platform::pasteboard` 快照恢复）。
