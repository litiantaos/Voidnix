# search

应用发现 + 文件搜索。Rust 只做数据召回（mdfind / app 扫描），过滤排序在前端 `src/utils/fuzzy.ts`。

## 缓存

- `APP_CACHE` 进程内全局 `RwLock`，双检锁懒加载；`prewarm_cache` 启动预热
- 先返回无图标列表，后台 `spawn_blocking` 提取图标后替换 cache 并 emit `app-cache-updated`
- `notify` 监听 `/Applications` 等目录变化（NonRecursive + 5s 防抖）后整体重建
- 会话内 `launch_app` 使用次数走 `SEARCH_SESSION.session_use_deltas`（内存 HashMap），重建时合并回 `use_count`

## 应用扫描

`mdfind "kMDItemContentType == 'com.apple.application-bundle' ..."` 扫 `/Applications`、`/System/Applications`、`~/Applications`；`scan_apps_from_dir`（递归 .app，深度 5）兜底。元数据优先 `mdls`，回退 `Info.plist`。

## 图标缓存

`~/Library/Caches/<bundle-id>/extensions/search/icons/`，文件名 `{hash(path+mtime)}.png`（应用更新自动失效）。启动时 `cleanup_icon_cache(400, 90)`：先删 90 天过期，再按 mtime 裁到 400 个。
