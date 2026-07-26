# Voidnix 官网

单页落地页，独立 Astro 子项目。视觉即产品——直接复用主应用 `src/styles/theme.css` 的 token 子集，纯 CSS 复刻启动器界面，无外部截图依赖。

## 目录

```
site/
├── astro.config.mjs          # 静态输出，inlineStylesheets: auto
├── public/
│   ├── favicon.png           # 站点图标
│   └── og-image.png          # 1200×630 社交分享图（脚本生成）
├── scripts/
│   ├── og-source.html        # OG 源稿（浏览器渲染 1200×630）
│   └── render-og.mjs         # Playwright 截图脚本
└── src/
    ├── components/           # Hero / Philosophy / Capabilities / ExtensionMatrix / Download / Footer / Wordmark
    ├── data/extensions.ts    # 21 扩展元数据 + 领域分簇
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

token 取自产品 `theme.css` 子集（`src/styles/tokens.css`）：基元 `--cool` / `--shadow-ink` / 缓动，面 `soft-surface` / `soft-card`，阴影 `card` / `panel` / `float`，圆角 `ctrl 6` / `panel 10` / `window 16`。改视觉先改 token，业务禁堆零散值。

## 部署

静态输出，`dist/` 可直接托管。已接入 Vercel（GitHub 集成自动部署）：

- **触发**：`git push origin main`（仅 `site/` 内改动）自动触发 Vercel 构建
- **域名**：`https://voidnix.litiantao.com`（CNAME 指向 Vercel）
- **配置**：`vercel.json`（Astro / `astro build` / `dist`）
- 项目地址：https://vercel.com/litiantao/voidnix

本地预览构建产物用 `bun run preview`。
