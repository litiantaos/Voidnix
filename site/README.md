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
    ├── components/           # Hero / DemoStage / Philosophy / Capabilities / ExtensionMatrix / Footer / Wordmark
    ├── data/extensions.ts    # 22 扩展元数据 + 领域分簇
    ├── layouts/BaseLayout.astro
    ├── pages/index.astro     # 单页章节编排
    └── styles/{tokens,global}.css
```

## 开发

```bash
bun install
bun run dev        # 本地预览
bun run build      # 产出 dist/
bun run preview    # 预览构建产物
```

类型检查需另装：`bun add -d @astrojs/check typescript` 后 `bun run check`。

## 重新生成 OG 图

`og-image.png` 由 Playwright 渲染 `scripts/og-source.html` 截图得到，依赖主仓库已安装的 `playwright` 与 chromium：

```bash
node scripts/render-og.mjs
```

改了 OG 源稿或 Wordmark 后重跑一次。

## 设计

token 从产品 `src/styles/theme.css` 自动同步（`scripts/sync-tokens.mjs`）：`dev` / `build` 前置自动运行，提取 `:root` + `:root[data-theme="dark"]` 的 CSS 自定义属性，产品深色选择器转为官网 `@media (prefers-color-scheme: dark)`。官网专属 token（布局 `--content-max` / DemoStage mock 场景 / 雾团透明度补偿）在 `tokens.css` 末尾 SITE_ONLY 区块维护。产品改色/圆角/阴影，官网自动跟。

## 部署

静态输出，`dist/` 可直接托管。已接入 Vercel（GitHub 集成自动部署）：

- **触发**：`git push origin main`（仅 `site/` 内改动）自动触发 Vercel 构建
- **域名**：`https://voidnix.litiantao.com`（CNAME 指向 Vercel）
- **配置**：`vercel.json`（Astro / `astro build` / `dist`）
- 项目地址：https://vercel.com/litiantao/voidnix

本地预览构建产物用 `bun run preview`。
