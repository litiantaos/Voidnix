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

设置 UI：「翻译服务」下列出两项；AI 弹窗多选中枢选用（单 Key 仅模型名，多 Key 主文案 `模型 · 备注`、字段名「模型与 Key」）、编辑提示词，并提供「管理提供商 / 打开 AI 提供商」跳转中枢扩展。无独立「模型」分组、无服务右侧加号。

`resolveAiTargets` / UI 摘要走 `effectiveAiSelections`（读时按中枢过滤无效选用 + 补 keyId）；`keyId` 缺省取第一把非空 Key；缺项可用 env 补。启动与 `updateAiConfig` 冷 prune 写回。运行结果引擎标签：单 Key 仅提供商名，多 Key 为「提供商 · 备注」。详见 [ai-providers.md](./ai-providers.md)。旧 AI 引擎字段启动时一次性导入中枢并 strip。

## 后端

- `native/mod.rs`：Extension trait + shortcut hook 注册 + `get_selected_text_cached` 命令（SELECTED_TEXT 自管，不泄漏框架）
- `native/ai_translate.rs`：AI 流式翻译（复用 `runtime::llm` 基础设施）
- `native/youdao.rs`：有道翻译（SHA256 签名）
- `native/lang_utils.rs`：中英检测 + smart_target_lang

划词取词流程：快捷键触发 → `platform::selection::try_ax()`（AX 直取）→ 失败则 `platform::input::post_combo("cmd+c", Some(pid))` → `platform::selection::poll_clipboard(snap)`（轮询 + `platform::pasteboard` 快照恢复）。
