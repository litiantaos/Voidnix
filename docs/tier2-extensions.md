# 开发 Tier 2 扩展

纯 JS/TS，零构建工具，一个文件即可。无 HTML/Vue，UI 由宿主用 5 个声明式原语独占渲染。Web Worker 沙箱 + CSP 锁定，只能通过 host API 访问系统能力。

## 包格式

```
my-ext.vnext/
├── manifest.toml         # 必需
├── index.js              # 必需，单文件 ESM
├── README.md             # 可选
├── i18n/                 # 可选
└── assets/               # 可选
```

## manifest.toml

```toml
[extension]
id = "my-ext"
name = "我的扩展"
version = "1.0.0"
description = "描述"
author = "作者"
icon = "i-ri-puzzle-line"
keywords = ["kw"]
voidnix_api = "^1"

[entry]
main = "index.js"

[capabilities]
required = ["clipboard.write"]
optional = ["storage", "http"]

[ui]
preferred_view = "list"          # list / markdown / form / detail / stream
search_placeholder = "输入内容"

# [settings]    # 可选：扩展设置项
# [shortcuts]   # 可选：快捷键
# [signature]   # 可选：签名验证
```

## 模块协议

```typescript
export default {
  id: string,
  onInit?(ctx): void | Promise<void>,
  onActivate?(ctx): void | Promise<void>,
  onDeactivate?(ctx): void | Promise<void>,
  onSearch?(query, ctx): View | Promise<View>,
  view?(): View | Promise<View>,
  onAction?(actionId, payload: { item?, form?, text? }, ctx): void | Promise<void>,
  subviews?: Record<string, (payload?) => View | Promise<View>>,
}
```

`export default` 前可声明顶层变量/函数（setup code），Worker bootstrap 自动分离。

**list 视图 execute 默认语义**：item 未声明 `actions` 数组且 `title` 非空时，框架直接复制 title 并隐藏窗口，不转发 worker `onAction`。扩展如需自定义 execute 行为，给 item 声明 `actions` 数组（DeclarativeList 会以上抛 primary 或首项 action.id 代替 `'execute'`，框架不再拦截），或把 `title` 留空让框架回落到转发 `onAction`。其他视图（form/detail/markdown/stream）的 action 一律转发到 worker。

## host API

Capability 在 manifest 声明，运行时按需注入，未声明为 `undefined`。

```typescript
ctx.ui.hide()                          // 隐藏窗口
ctx.ui.setView(view)                   // Push 模式更新 UI
ctx.clipboard.write(text)              // 复制到剪贴板（需 clipboard.write）
ctx.http.fetch(url, init)              // HTTP 请求（需 http）
ctx.storage.get(key) / .set(key, val)  // 持久存储（需 storage）
```

## 声明式 UI 原语

扩展返回 View 描述对象，宿主 `DeclarativeHost` 独占渲染：

- `list`：标题 + 副标题 + 图标 + actions
- `markdown`：富文本
- `form`：类型化输入字段
- `detail`：主体 + 侧栏元数据
- `stream`：append-only 流式 markdown

类型定义：`src/types/declarative.ts`，组件：`src/components/declarative/`

## 沙箱架构

Worker 通过 Blob URL 创建，CSP 锁定（无 DOM/网络）。宿主 `worker-sandbox.ts` 代理所有 host API 调用，JSON-RPC 2.0 over `postMessage`。`tier2-registry.ts` 将 Tier 2 扩展桥接为 `AppModule` 适配器。

- Worker 生命周期：首次调用 spawn，5 分钟未活跃 terminate，禁用/卸载立即 terminate
- CSP：`worker-src 'self' blob:`；Capability 强制：安装时检查 required，运行时只注入已声明的 API
- ID 与 Tier 1 冲突时 Tier 2 被跳过

## 加载与开发

- **生产路径**：`~/Library/Application Support/com.litiantao.voidnix/extensions/<id>/`
- **开发加载**：debug 构建自动扫描项目 `extensions/*.vnext/`；release 构建设置 `VOIDNIX_DEV_EXTENSIONS` 环境变量
- **正式版测试开发扩展**：`VOIDNIX_DEV_EXTENSIONS=~/Code/Voidnix/extensions /Applications/Voidnix.app/Contents/MacOS/Voidnix`
- **安装/卸载**：`ext_install`（zip 包）/ `ext_uninstall` 实时生效无需重启
