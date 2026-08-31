# Voidnix 官网

单页落地页，独立 Astro 子项目。视觉即产品——token 直接从主应用 `src/styles/theme.css` 自动同步（`scripts/sync-tokens.mjs`），纯 CSS 复刻启动器界面，无外部截图依赖。

## 目录

```
site/
├── astro.config.mjs          # 静态输出，inlineStylesheets: auto
├── public/
│   ├── favicon.png           # 站点图标
│   └── og-image.png          # 1200×630 社交分享图（脚本生成）
├── scripts/
│   ├── sync-tokens.mjs      # 产品 theme.css → tokens.css token 同步（dev/build 前置）
│   ├── capture-demo.mjs     # Demo 动画逐帧捕获 → MP4/WebM（可选，社交分享用）
│   ├── og-source.html        # OG 源稿（浏览器渲染 1200×630）
│   └── render-og.mjs         # Playwright 截图脚本
└── src/
    ├── components/           # PageContent / Hero / DemoStage / Philosophy / Capabilities / ExtensionMatrix / Footer / Wordmark
    │   └── demo/             # DemoStage 实现主体（demo-utils.ts 常量与数学工具 / demo-scenes.ts 分段渲染器 / demo-player.ts 播放器 / demo-stage.css 舞台样式）
    ├── data/extensions.ts    # 扩展矩阵便捷访问器（数据在 i18n 字典）
    ├── i18n/
    │   ├── translations.ts   # 页面级双语字典（zh 类型源，en 同构校验）
    │   └── demo.ts           # Demo 动画双语文案
    ├── layouts/BaseLayout.astro
    ├── pages/
    │   ├── index.astro       # 中文首页（/）
    │   ├── demo.astro        # 中文 demo（/demo）
    │   └── en/               # 英文路由（/en/、/en/demo）
    └── styles/{tokens,global}.css
```

## 国际化（i18n）

中英双语，URL 前缀路由：`/`（中文，默认）与 `/en/`（英文）。

**架构**：

- `src/i18n/translations.ts` — 页面级文案单一源。`zh` 对象为类型源，`en: typeof zh` 编译期保证完整性。`getDict(lang)` 返回该语言全部文案。
- `src/i18n/demo.ts` — Demo 动画专属文案（字幕 / 搜索框 / Agent 对话 / 控制按钮等），浏览器端按 `data-lang` 属性取值。
- 各组件接收 `lang` prop，内部 `const t = getDict(lang)` 取文案。
- DemoStage 设 `data-lang` 属性，demo-scenes / demo-player 读此属性选择语言。
- 语言切换：Hero nav 内链接，中文页显示「EN」指向 `/en/`，英文页显示「中文」指向 `/`。
- SEO：`<html lang>` / `og:locale` / `hreflang` 双语互指均按语言输出。

**新增/修改文案**：统一改 `translations.ts`（页面级）或 `demo.ts`（动画级）。`en` 缺 key 时 TS 报错。

## 开发

```bash
bun install
bun run dev        # 本地预览
bun run build      # 产出 dist/
bun run preview    # 预览构建产物
```

类型检查需另装：`bun add -d @astrojs/check typescript` 后 `bun run check`。

## 重新生成 OG 图

`og-image.png` 由 Playwright 对 `scripts/og-source.html` 截图得到，`playwright` 是 site 自身 devDependency（独立 bun.lock / node_modules，`bun install` 即装），另需已安装 chromium：

```bash
node scripts/render-og.mjs
```

改了 OG 源稿或 Wordmark 后重跑一次。

## 设计

token 从产品 `src/styles/theme.css` 自动同步（`scripts/sync-tokens.mjs`）：`dev` / `build` 前置自动运行，提取 `:root` + `:root[data-theme="dark"]` 的 CSS 自定义属性，产品深色选择器转为官网 `@media (prefers-color-scheme: dark)`。官网专属 token（布局 `--content-max` / DemoStage mock 场景 / 雾团透明度补偿）在 `tokens.css` 末尾 SITE_ONLY 区块维护。

排版：正文走系统 sans（SITE_ONLY 覆写 `--font-sans`，不继承产品 mono 优先）；技术性内容显式 `--font-mono`——快捷键标签、路径与文件名、终端与命令输出、尺寸标注、数值徽标、Hero 规格行。

## 部署

静态输出，`dist/` 可直接托管。已接入 Vercel（GitHub 集成自动部署）：

- **触发**：`git push origin main`（仅 `site/` 内改动）自动触发 Vercel 构建
- **域名**：`https://voidnix.litiantao.com`（CNAME 指向 Vercel）
- **配置**：`vercel.json`（Astro / `astro build` / `dist`）
- 项目地址：https://vercel.com/litiantao/voidnix
