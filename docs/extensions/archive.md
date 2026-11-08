# archive（压缩 / 解压）

zip / 7z 压缩（系统垃圾 / 隐藏文件过滤、AES-256 密码）+ 多格式解压（RAR 含加密档、ISO 镜像）（包内预览、取消、Zip-Slip 防护）。

## 能力与入口

- **入口**：访达多选文件/文件夹 → `⌥F` →「压缩 / 解压」行（CustomEvent `archive-pending-input-path` 同页同步投递，全归档选区 → 解压模式、其余 → 压缩模式）；扩展内空态为两行：操作方式（解压/压缩，默认解压）+ 选文件（解压 = 单选压缩包 / 压缩 = 多选任意文件），选完进入与访达入口同构的对应界面
- **压缩**（BaseSettingsList 表单）：源行（N 项 + 首名预览 + 行尾按钮组「压缩 / busy 状态机」，不展示待压缩列表）、归档名称（回车即压缩）、格式 ZIP/**7Z**（7z 走 sevenz-rust crate，LZMA2；**RAR 创建不可支持**——编码器专有，开源生态仅能解压；曾支持 TAR.GZ 已下线）、级别预设三档（最快=1 / 标准=6 / 最小=9，zip 映射 Deflated level / 7z 映射 LZMA2 preset，选择记忆到 config）、密码（AES-256，默认格式 ZIP；经弹窗两次输入设置——密码行回车亦可打开，不一致报错、均留空确定即清除，行尾按钮显「已设置/未加密」，不落盘）、过滤开关（系统垃圾 + 隐藏文件，**均不落盘**——每次进入压缩界面重置默认：垃圾过滤开 / 隐藏过滤关）；**无进度条**——busy 态按钮 loading 文案 + 取消，完成 toast + 「在 Finder 中显示」；单文件根级直存（不包一层目录）、符号链接按链接条目保留（7z 无链接语义、按目标内容直存）；输出防覆盖（`名 2.zip` 递增）；过滤剔除——系统垃圾（`.DS_Store` / `__MACOSX/` / `._*` / `.Spotlight-V100` / `.fseventsd` / `.Trashes` / `.TemporaryItems` / `Thumbs.db` / `desktop.ini`）与隐藏条目（点开头组件，含 `.git`；默认关），`native/junk.rs` 纯函数
- **加密（AES-256）**：zip 写入 `with_aes_encryption`（目录条目不加密——Finder/bsdtar 兼容差），解压/预览走 `by_index_decrypt`；RAR 加密档（-p 内容加密无密码可列 / -hp 档头加密列表失败退化为已加密+空条目）经 unrar crate 解密，错密码在 unrar 层报 CRC 类错误、经前置加密探测统一映射「密码错误」；7z 写入 sevenz-rust `AesEncoderOptions`（**档头不加密**——条目名无密码可见，与 zip 语义一致），解压/预览同 crate 解密（bsdtar 无 `--passphrase` 传不了密码，故 7z 读路径全部走 sevenz-rust 原生而非 bsdtar）；密码缺失/错误分别报「需要密码」/「密码错误」（7z 空密码解密产垃圾流报 Corrupted，前置探测 AES coder 显式拦截映射文案）；密码只在请求体单次传递**不落盘**
- **解压**（BaseSettingsList 表单三行）：压缩包行（文件名 + 格式/条数/大小副标题，行尾「解压」按钮，回车同触发）+ 解压到行（默认**压缩包同目录**，可另选可重置，镜像 video 输出目录范式；会话级不落盘）+ 「查看压缩包」行（回车进入 `entries` 二级界面）+ 加密包密码行（内联输入，回车即解压）；zip / tar.gz / 7z / **rar（unrar crate：RarLab unRAR C++ 源码静态编译，支持 RAR3/RAR5 加密档解密；创建侧因 unRAR 许可禁止仍不可行）** 走 crate 直读；tar.bz2 / tar.xz / tar.zst / 裸 tar / **iso（iso9660 镜像；魔数 `CD001` 在 0x8001 偏移，format::detect 二次探测）** 走系统 bsdtar（Apple 定制 bsdtar 无 `--passphrase`，故 RAR 从 bsdtar 切至 unrar）；恒解压到 `<目标目录>/<压缩包名>/`（冲突递增），多顶层条目不喷洒用户目录；**无进度条**（同压缩）
- **文件树二级界面**（`TreeView.vue`，subview `entries`）：扁平条目构建目录树（排序：目录在前、字典序；首层默认展开），紧凑单行（镜像 proxy 连接列表 p-3 text-xs 范式，目录行尾显直接子项数）；↑↓ 移动、→/回车展开、← 收起、文件行**回车开/关预览**（唯一通道；覆盖层与 clipboard 预览同结构——纯内容居中无信息栏，跟随高亮刷新，加密包带共享密码）；**顶部搜索栏可输入过滤**（命中节点平铺；disableSearchInput 扩展在 subview 中放开 readonly——MainView 通用规则，进入自动聚焦、离开清词）；条目多经 max-h + 内部滚动封顶

## 架构

```
native/
├── format.rs   # magic bytes 探测（PK\x03\x04 / Rar! / 7z¼¯' / gzip/xz/bz2/zstd 头 / ISO@0x8001）+ 扩展名兜底
├── junk.rs     # 垃圾过滤表（纯函数）
├── inspect.rs  # 列表：zip 随机访问 / tar.gz 顺序扫描 / bsdtar `tar -tf` 纯文件名（cap 5000）
├── preview.rs  # 单条目 ≤2MB 流读 + NUL 嗅探二进制 + 图片扩展名识别
├── extract.rs  # crate 路径逐 entry（Slip 校验 + 进度）与 bsdtar 路径（-- 操作数 + 可 kill）
├── compress.rs # walkdir 收集 → 过滤（垃圾/隐藏） → zip/7z 写出
├── sevenz.rs   # 7z 读路径（列表/解压/单条目读取 + AES 解密；写入在 compress.rs）
├── rar.rs      # RAR 读路径（unrar crate：加密档解密 + 密码文案映射；testdata/ 为官方 fixtures）
├── bsdtar.rs   # /usr/bin/tar 进程封装（.output() / 断管限流读 / try_wait 轮询取消）
├── job.rs      # 单飞任务槽 + CancellationToken + Channel 单投递
└── mod.rs      # 命令：archive_inspect / archive_preview / archive_extract / archive_compress / archive_cancel
```

## 安全边界

- **Zip-Slip**：crate 路径 zip 用 `enclosed_name()`（绝对路径 / `..` → 拒绝计数）；tar 自实现 `sanitize_rel`（拒 ParentDir/RootDir）；符号链接目标必须为包内相对路径（绝对或 `..` 一律拒）。恶意条目不中断任务，`skippedUnsafe` 计数回传 UI 提示
- **bsdtar 选项注入**：条目名一律经 `--` 操作数传递（`-` 开头的恶意条目名不可能被解析为 tar 选项）；libarchive 自身默认拒绝绝对路径与 `..` 逃逸
- **bsdtar 名称转义**：不设 `LC_ALL=C`（C locale 会把中文文件名转义为 `\345...` 八进制），列表层剥 `./` 前缀归一
- **TCC**：输出目录恒经 `pick_directory`（用户选择即授权）或设置页固定目录；压缩输出路径在 Rust 侧防覆盖递增，不覆盖既有文件
- **spawn 纪律**：bsdtar 一次性任务 `.output()` 收尾；流式读满上限 / 取消时先断管或 kill 子进程，再 `wait()` 回收

## 取消与事件

`archive_extract` / `archive_compress` 共用单飞任务槽（`job::begin/end/cancel`，join 错误也 `end` 防 BUSY 泄漏）；crate 路径逐 entry 检查 CancellationToken，bsdtar 解压 `try_wait` 轮询并 kill 子进程（progress 事件仍投递但 UI 不消费——无进度条设计）。事件经本次 invoke 的 Channel 单投递；任务状态随视图会话（KeepAlive）保留，不跨视图重挂载 / 页面重载恢复。

## 配置（config subview）

`compressDir`（空 = 每次选择）/ `level`（压缩等级）。解压目录不落盘——默认压缩包同目录，可另选可重置。格式与过滤开关**不落盘**——每次进入压缩界面重置默认（zip / 垃圾过滤开 / 隐藏过滤关）。

## 测试

- Rust 内联：magic bytes 探测、垃圾过滤、**恶意 zip 往返**（`../evil`、绝对路径、逃逸符号链接全部拦截且不影响正常条目）、选择性解压子树、zip 压缩往返（垃圾过滤生效 + 单文件根条目名 + 符号链接保留）、bsdtar tar.xz 真实往返（中文+空格名 + 取消）、大条目流读不死锁、7z 明文/加密往返（加密无密码可列条目 + 错密码拒解 + 正确密码可解）
- 前端：`logic.test.ts`（归档扩展名识别 / 命名 / 输出扩展名规范化 / 字节格式化）
