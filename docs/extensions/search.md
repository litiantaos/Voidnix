# search

应用发现 + 文件搜索。Rust 只做数据召回（mdfind / app 扫描），过滤排序在前端 `src/utils/fuzzy.ts`。

## 缓存

- `APP_CACHE` 进程内全局 `RwLock`，双检锁懒加载；`prewarm_cache` 启动预热
- 先返回无图标列表，后台 `spawn_blocking` 提取图标后替换 cache 并 emit `app-cache-updated`
- `notify` 监听 `/Applications` 等目录变化（NonRecursive + 5s 防抖）后整体重建
- 会话内 `launch_app` 使用次数走 `SEARCH_SESSION.session_use_deltas`（内存 HashMap），重建时合并回 `use_count`

## 应用扫描

`mdfind "kMDItemContentType == 'com.apple.application-bundle' ..."` 扫 `/Applications`、`/System/Applications`、`~/Applications`；`scan_apps_from_dir`（递归 .app，深度 5）兜底。元数据优先 `mdls`，回退 `Info.plist`。

## 文件搜索

`search_files` 用 `mdfind -name <query> -onlyin ~` 单次拉候选，`-attr kMDItemContentType` / `-attr kMDItemUseCount` 顺带取类型与使用次数。

**零 TCC 原理**：家目录本身不受 TCC 保护；Spotlight 守护进程 `mds` 以系统权限索引所有文件（含 Documents/Desktop/Downloads 等受保护目录），故无需 FDA 即可搜到内容。Rust 端用 `starts_with` 在 reader 循环内按目标子目录前缀即时过滤——纯字符串匹配，不触达文件系统。

**白名单子目录**（`TARGET_SUBDIRS`）：Desktop / Documents / Downloads / Pictures / Music / Movies / Projects / Code。前缀过滤在解析阶段执行，保证 `MAX_ENTRIES=100` 配额全部留给目标子目录，不被家目录其它路径稀释。

**解析与配额**：`spawn_blocking` 内按空行/新路径分块切条目，命中前缀才入列；达到 `MAX_ENTRIES` 立即 break。整体超时 3s，超时 kill 子进程并返回错误。会话 id 守护：await 期间若有新查询进入，旧结果整批丢弃。

**类型与排序**：`kMDItemContentType` 含 `public.folder` 判为 folder，否则 file；使用次数透传给前端打分。前端 `src/utils/fuzzy.ts` 负责拼音匹配与排序（与 application 同通道）。

## 图标

实时提取（`NSWorkspace.iconForFile`），无磁盘缓存。系统自带 NSCache 加速，实测 <1ms/应用。启动时并行预热常用应用图标。
