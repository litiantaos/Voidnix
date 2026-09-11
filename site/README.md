# Voidnix 官网

单页落地页，独立 Astro 子项目。视觉即产品——token 直接从主应用 `src/styles/theme.css` 自动同步（`scripts/sync-tokens.mjs`），纯 CSS 复刻启动器界面，无外部截图依赖。

## 目录

```
site/
├── astro.config.mjs          # 静态输出，inlineStylesheets: auto
├── public/
│   ├── favicon.png           # 站点图标
│   ├── og-image.png          # 1200×630 社交分享图·中文（脚本生成）
│   └── og-image-en.png       # 同上·英文（render-og.mjs 注入 en 文案）
├── scripts/
│   ├── sync-tokens.mjs      # 产品 theme.css → tokens.css token 同步（dev/build 前置）
│   ├── capture-demo.mjs     # Demo 动画逐帧捕获 → MP4/WebM（可选，社交分享用）
│   ├── og-source.html        # OG 源稿（浏览器渲染 1200×630，右侧键盘图纸背景）
│   ├── gen-og-keyboard.mjs  # 首启引导等距键盘几何移植 → og-keyboard.svg（OG 背景素材）
│   └── render-og.mjs         # Playwright 截图脚本
└── src/
    ├── components/           # PageContent / Hero / DemoStage / Philosophy / Capabilities / ExtensionMatrix / InstallNotes / Footer / Wordmark
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

`og-image.png`（中文）与 `og-image-en.png`（英文，`render-og.mjs` 渲染前注入文案替换——英文标题经 `<br>` 两行，与 site i18n hero 对齐；`BaseLayout` 按语言引用）由 Playwright 对 `scripts/og-source.html` 截图得到，信息行两行纵排（desc / feats）为双语共用的源稿布局。背景是首启引导三维键盘图纸（大幅右置、墨迹贴画布右缘），叠加左→右透明渐变（CSS mask，起点保留 10% 浅水印不降至全透、右侧全显）：`gen-og-keyboard.mjs` 逐函数移植 `src/components/layout/WelcomeView.vue` 的等距几何（f=0 等距终态、默认 Alt 基键位），颜色读 `tokens.css` 烘焙，生成 `scripts/og-keyboard.svg`（生成物但提交，加入 `.prettierignore` 保持确定性）；改动键盘几何或默认键位时同步移植。一键重生成（含 token 同步，双语两图）：

```bash
bun run generate:og
```

`playwright` 是 site 自身 devDependency（独立 bun.lock / node_modules，`bun install` 即装），另需已安装 chromium。改了 OG 源稿或 Wordmark 后重跑一次。

## 设计

token 从产品 `src/styles/theme.css` 自动同步（`scripts/sync-tokens.mjs`）：`dev` / `build` 前置自动运行，提取 `:root` + `:root[data-theme="dark"]` 的 CSS 自定义属性，深色选择器与产品同构原样保留。官网专属 token（布局 `--content-max` / DemoStage mock 场景 / 雾团透明度补偿）在 `tokens.css` 末尾 SITE_ONLY 区块维护。

深色模式：`ThemeInit.astro` 内联脚本在首帧前解析三态偏好（`localStorage('voidnix-site-theme')`：auto 跟随系统 / light / dark）写入 `<html data-theme>`，Hero 导航栏按钮循环切换并联动 `theme-color` meta；无 JS 环境回退浅色。

排版：正文走内置 Noto Sans（`site/public/fonts/` 拉丁可变字体单文件，@font-face 在 `global.css`，SITE_ONLY 覆写 `--font-sans` 不继承产品 mono 优先），CJK 回退系统 PingFang SC；技术性内容显式 `--font-mono`——快捷键标签、路径与文件名、终端与命令输出、尺寸标注、数值徽标、Hero 规格行（DemoStage 启动器 mock 复刻产品 mono，由 `demo-stage.css` 显式声明）。

## 部署

静态输出，`dist/` 可直接托管。已接入 Vercel（GitHub 集成自动部署）：

- **触发**：`git push origin main`（仅 `site/` 内改动）自动触发 Vercel 构建
- **域名**：`https://voidnix.app`（CNAME 指向 Vercel）
- **配置**：`vercel.json`（Astro / `astro build` / `dist`）
- 项目地址：https://vercel.com/litiantao/voidnix
