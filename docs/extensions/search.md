# search

应用发现 + 文件搜索。Rust 维护两个内存索引（`APP_CACHE` + `FILE_CACHE`），过滤排序在前端 `src/utils/fuzzy.ts`。

## 缓存

- `APP_CACHE` / `FILE_CACHE` 进程内全局 `RwLock`，双检锁懒加载；`prewarm_cache` 启动并发预热两者
- 应用先返回无图标列表，后台 `spawn_blocking` 提取图标后替换 cache 并 emit `app-cache-updated`
- `notify` 监听 `/Applications` + 文件扫描目录变化（FSEvents 递归 + 5s 防抖）后并发重建两个缓存
- 会话内 `launch_app` 使用次数走 `SEARCH_SESSION.session_use_deltas`（内存 HashMap），重建时合并回 `use_count`

## 应用扫描

`mdfind "kMDItemContentType == 'com.apple.application-bundle' ..."` 扫 `/Applications`、`/System/Applications`、`~/Applications`；`scan_apps_from_dir`（递归 .app，深度 5）兜底。元数据优先 `mdls`，回退 `Info.plist`。

## 文件搜索

`search_files` 对 `FILE_CACHE` 内存索引做 substring 匹配 + 基础打分（前缀 1000 / 包含 600 / 位置扣分），返回 top 100 候选。不再 per-query spawn mdfind——随打随出。

**索引构建**：启动时 `spawn_blocking` 递归扫描目标子目录，跳过隐藏文件 + `FILE_IGNORE_DIRS`（node_modules / .git / dist / build / target 等），深度上限 6 层，数量上限 50,000。

**白名单子目录**（`FILE_SCAN_DIRS`）：Desktop / Documents / Downloads / Pictures / Music / Movies / Projects / Code。

**name_lower 预计算**：扫描时 `to_lowercase` 一次，搜索时零分配 `String::find`。

**use_count / last_used**：索引构建时一次 `mdfind "kMDItemUseCount > 0"` 批量拉目标目录下被打开过的文件元数据（远少于全量），合并进 `CachedFile`。`last_used` 的 Spotlight 日期字符串经 `parse_epoch_hours`（Howard Hinnant days-from-civil）预解析为 epoch hours 存入 `last_used_hours`，搜索时纯整数减法算 hours_ago 做 recency 分桶——零日期解析热路径开销。

**排序权重**（Rust 端截断 top 100 时用，与前端 `frequencyBoost`/`recencyScore` 对齐）：substring（前缀 1000 / 包含 600）+ frequency（log2 平滑 cap 1500）+ recency（<1h=300 / <24h=200 / <168h=100 / <720h=50）+ folder 优先 240。确保高频/近期文件在截断时不被丢弃，前端 `scoreFields` 再做 fuzzy + boost 精排。

## 图标

实时提取（`NSWorkspace.iconForFile`），无磁盘缓存。系统自带 NSCache 加速，实测 <1ms/应用。启动时并行预热常用应用图标。
