# translate

选中文本翻译 + 输入即译。全局快捷键抓取选中文本（Accessibility API / 剪贴板兜底），并行调用多引擎并排展示。目标语言自动反转（中文→英文，反之亦然）。无翻译历史落库，配置走全局 `useSettingsStore`。

## 后端

- **有道**（`youdao.rs`）：`openapi.youdao.com/api`，POST 表单 + SHA-256 签名（signType v3），非流式
- **AI**（`ai_translate.rs`）：OpenAI 兼容 `/chat/completions`，Bearer Token，支持 SSE 流式（`translate_ai_stream` 走 `sse::stream_openai_request`，`translate-chunk`/`translate-done` 事件推送）

前端遍历配置并行请求，AI 类型可按 model × config 展开多行。结果清洗（去代码块/引号/前导语）Rust（`clean_translation`）和 TS（`cleanStreamResult`）两侧各实现一遍。选中文本抓取仅 macOS（`#[cfg(target_os = "macos")]`）。
