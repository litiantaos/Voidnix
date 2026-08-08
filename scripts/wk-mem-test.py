#!/usr/bin/env python3
"""Voidnix WebKit 内存累积测试。

模拟真实使用模式覆盖全路径：
  全局搜索（应用 / 文件 / 即时答案 / web / 工具列表）
  → 结果键盘导航
  → 进入全部 mainView 扩展视图 + 搜索型扩展内交互
  → 全局快捷键（截屏 overlay 独立窗口 + 扩展唤起 Alt+C/T/A/F）
  → 窗口管理 snap-panel 鼠标触发
  → hide/show 循环释放测量

分阶段测量主 WebContent 进程的 Physical footprint + graphics 区域累积（PURGE=N 不可回收层）。
测试前自动切 ASCII 键盘布局（HIToolbox TIS），测完恢复。

用法：
    python3 scripts/wk-mem-test.py              # 默认 5 轮全场景
    python3 scripts/wk-mem-test.py 10           # 10 轮
    python3 scripts/wk-mem-test.py 5 --dev      # dev 构建（快捷键叠加 Shift）

依赖：pyobjc-framework-Quartz（macOS 自带系统 Python 已含）
"""

import Quartz
import subprocess
import time
import sys
import re
import ctypes

# ── 参数 ─────────────────────────────────────────────────────────────────────

ROUNDS = 5
for arg in sys.argv[1:]:
    if arg == '--dev':
        continue
    try:
        ROUNDS = int(arg)
    except ValueError:
        pass

DEV_MODE = '--dev' in sys.argv

# ── 输入法管理（HIToolbox TIS API）────────────────────────────────────────────
# 中文输入法拦截 CGEvent 按键：拼音组合态下逐字符被缓冲成连续拼音串而非进入搜索框；
# 扩展内含空格的输入（如 "100 eur"）空格被当选词确认。测试前切 ASCII 布局，测完恢复。

_HITOOLBOX = None
try:
    _HITOOLBOX = ctypes.cdll.LoadLibrary(
        '/System/Library/Frameworks/Carbon.framework/Frameworks/HIToolbox.framework/HIToolbox'
    )
    _HITOOLBOX.TISCopyCurrentKeyboardInputSource.restype = ctypes.c_void_p
    _HITOOLBOX.TISCopyCurrentASCIICapableKeyboardLayoutInputSource.restype = ctypes.c_void_p
    _HITOOLBOX.TISSelectInputSource.argtypes = [ctypes.c_void_p]
    _HITOOLBOX.TISSelectInputSource.restype = ctypes.c_int
except OSError:
    pass


def switch_to_ascii():
    """切到 ASCII 键盘布局（ABC），返回原输入源指针供恢复。"""
    if not _HITOOLBOX:
        return None
    saved = _HITOOLBOX.TISCopyCurrentKeyboardInputSource()
    ascii_src = _HITOOLBOX.TISCopyCurrentASCIICapableKeyboardLayoutInputSource()
    if ascii_src:
        _HITOOLBOX.TISSelectInputSource(ascii_src)
    return saved


def restore_input_source(saved):
    if saved and _HITOOLBOX:
        _HITOOLBOX.TISSelectInputSource(saved)

# ── 时序常量 ──────────────────────────────────────────────────────────────────

TYPE_DELAY = 0.045        # 每字符间隔（模拟真实快打）
SETTLE_INSTANT = 0.6      # 计算器 / 应用缓存（同步毫秒级）
SETTLE_DEFAULT = 1.0      # 普通搜索（文件索引 ~3ms，扩展 dynamic）
SETTLE_NETWORK = 2.5      # 网络（汇率 / IP 信息）
NAV_DELAY = 0.12          # 方向键之间
ESC_DELAY = 0.3           # Escape 退出扩展后
TOGGLE_GAP = 1.0          # toggle_window 后等待窗口拿到键盘焦点


# ── 窗口状态追踪 ──────────────────────────────────────────────────────────────
# Voidnix 是 Accessory app（不 activate），窗口隐藏后键盘焦点回到前台 app。
# CGEvent 是全局 HID 事件，发到当前 key window——如果 Voidnix 窗口不可见，
# 按键会打到终端 / IDE。因此必须精确追踪窗口可见性，绝不盲发按键。

_win_visible = False   # Voidnix 窗口是否可见
_in_ext = False        # 是否在扩展视图内（决定 Esc 语义）


def log(msg: str):
    print(msg, flush=True)


# ── 字符 → 键码映射 ───────────────────────────────────────────────────────────

SHIFT = Quartz.kCGEventFlagMaskShift
CMD = Quartz.kCGEventFlagMaskCommand
ALT = Quartz.kCGEventFlagMaskAlternate

_BASE = {
    'a': 0, 'b': 11, 'c': 8, 'd': 2, 'e': 14, 'f': 3, 'g': 5, 'h': 4,
    'i': 34, 'j': 38, 'k': 40, 'l': 37, 'm': 46, 'n': 45, 'o': 31,
    'p': 35, 'q': 12, 'r': 15, 's': 1, 't': 17, 'u': 32, 'v': 9,
    'w': 13, 'x': 7, 'y': 16, 'z': 6,
    '1': 18, '2': 19, '3': 20, '4': 21, '5': 23, '6': 22, '7': 26,
    '8': 28, '9': 25, '0': 29,
    '-': 27, '=': 24, '[': 33, ']': 30, '\\': 42, ';': 41,
    "'": 39, '`': 50, ',': 43, '.': 47, '/': 44, ' ': 49,
}

_SHIFTED = {
    '+': 24, '_': 27, '{': 33, '}': 30, '|': 42, ':': 41,
    '"': 39, '~': 50, '<': 43, '>': 47, '?': 44,
    '!': 18, '@': 19, '#': 20, '$': 21, '%': 23, '^': 22,
    '&': 26, '*': 28, '(': 25, ')': 29,
}

KEY_ENTER = 36
KEY_ESC = 53
KEY_TAB = 48
KEY_BACKSPACE = 51
KEY_DOWN = 125
KEY_UP = 126


def _char_to_keycode(ch: str):
    if ch in _BASE:
        return (_BASE[ch], 0)
    lc = ch.lower()
    if ch.isupper() and lc in _BASE:
        return (_BASE[lc], SHIFT)
    if ch in _SHIFTED:
        return (_SHIFTED[ch], SHIFT)
    return None


def _post_key(keycode: int, flags: int = 0):
    down = Quartz.CGEventCreateKeyboardEvent(None, keycode, True)
    Quartz.CGEventSetFlags(down, flags)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, down)
    up = Quartz.CGEventCreateKeyboardEvent(None, keycode, False)
    Quartz.CGEventSetFlags(up, flags)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)


# ── 窗口操作（状态安全）──────────────────────────────────────────────────────

def _ext_flags():
    """扩展快捷键修饰符（Alt 基，dev 叠加 Shift）。"""
    flags = ALT
    if DEV_MODE:
        flags |= SHIFT
    return flags


def _shortcut_press():
    """Option+Space（dev 叠加 Shift）。"""
    _post_key(49, flags=_ext_flags())


def trigger_ext_shortcut(key_char: str):
    """触发扩展全局快捷键（Alt+key，dev 叠加 Shift）。"""
    kc = _char_to_keycode(key_char.lower())
    if kc:
        _post_key(kc[0], flags=_ext_flags())


def show_window():
    """确保窗口可见。仅在认为隐藏时 toggle。"""
    global _win_visible
    if _win_visible:
        return
    _shortcut_press()
    time.sleep(TOGGLE_GAP)
    _win_visible = True


def hide_window():
    """确保窗口隐藏。仅在认为可见时 toggle。"""
    global _win_visible
    if not _win_visible:
        return
    _shortcut_press()
    time.sleep(TOGGLE_GAP)
    _win_visible = False


# ── 输入操作（仅在窗口可见时调用）────────────────────────────────────────────

def type_text(s: str, delay: float = TYPE_DELAY):
    for ch in s:
        kc = _char_to_keycode(ch)
        if kc:
            _post_key(kc[0], flags=kc[1])
            time.sleep(delay)


def press_enter():
    _post_key(KEY_ENTER)


def press_esc():
    _post_key(KEY_ESC)


def press_backspace():
    _post_key(KEY_BACKSPACE)


def press_down(n: int = 1, delay: float = NAV_DELAY):
    for _ in range(n):
        _post_key(KEY_DOWN)
        time.sleep(delay)


def press_up(n: int = 1, delay: float = NAV_DELAY):
    for _ in range(n):
        _post_key(KEY_UP)
        time.sleep(delay)


def select_all():
    _post_key(_BASE['a'], flags=CMD)


def clear_input():
    """Cmd+A → Backspace 清空搜索框（仅在窗口可见时安全）。"""
    select_all()
    time.sleep(0.04)
    press_backspace()
    time.sleep(0.08)


def search_and_wait(query: str, settle: float = SETTLE_DEFAULT):
    """清空 → 输入 → 等待结果渲染。"""
    clear_input()
    type_text(query)
    time.sleep(settle)


def enter_ext(name: str, settle: float = SETTLE_DEFAULT):
    """进入扩展：/name → Enter 激活。"""
    global _in_ext
    clear_input()
    type_text('/' + name)
    time.sleep(SETTLE_INSTANT)
    press_enter()
    time.sleep(settle)
    _in_ext = True


def exit_ext():
    """退出扩展：Esc（仅当在扩展内时安全；全局模式下 Esc 会隐藏窗口）。"""
    global _in_ext
    if not _in_ext:
        return
    press_esc()
    time.sleep(ESC_DELAY)
    _in_ext = False


# ── 鼠标操作 ──────────────────────────────────────────────────────────────────

def move_mouse(x: float, y: float):
    """移动鼠标到全局坐标 (x, y)，原点在主屏左上角（CGEvent 坐标系）。"""
    point = Quartz.CGPoint(x, y)
    event = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, point, 0)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, event)


def screen_size() -> tuple:
    """返回主屏像素尺寸 (width, height)。"""
    did = Quartz.CGMainDisplayID()
    w = Quartz.CGDisplayPixelsWide(did)
    h = Quartz.CGDisplayPixelsHigh(did)
    return (w, h)


# ── 内存测量 ──────────────────────────────────────────────────────────────────

def find_main_webcontent_pid() -> int:
    out = subprocess.check_output(['ps', '-A', '-o', 'pid=,rss=,comm='], text=True)
    candidates = []
    for line in out.splitlines():
        if 'WebKit.WebContent.xpc' in line and 'EnhancedSecurity' not in line:
            parts = line.strip().split()
            if len(parts) >= 3:
                candidates.append((int(parts[0]), int(parts[1])))
    if not candidates:
        raise RuntimeError('未找到 WebContent 进程，Voidnix 是否已启动？')
    candidates.sort(key=lambda x: x[1], reverse=True)
    return candidates[0][0]


def measure_footprint(pid: int) -> str:
    out = subprocess.check_output(
        ['vmmap', '--summary', str(pid)], text=True, stderr=subprocess.DEVNULL
    )
    for line in out.splitlines():
        if 'Physical footprint:' in line and 'peak' not in line:
            return line.strip().split(':')[1].strip()
    return '?'


def measure_graphics(pid: int) -> dict:
    out = subprocess.check_output(['vmmap', str(pid)], text=True, stderr=subprocess.DEVNULL)
    regions = []
    for line in out.splitlines():
        if 'owned unmapped (graphics)' not in line:
            continue
        m = re.search(
            r'\[\s*([\d.]+)([KM]?)\s+([\d.]+)([KM]?)\s+([\d.]+)([KM]?)\s+([\d.]+)([KM]?)\s*\]',
            line,
        )
        if not m:
            continue

        def to_kb(val, unit):
            return float(val) * 1024 if unit == 'M' else float(val)

        resident = to_kb(m.group(3), m.group(4))
        swapped = to_kb(m.group(7), m.group(8))
        purge = (
            'N' if 'PURGE=N' in line
            else 'V' if 'PURGE=V' in line
            else 'E'
        )
        regions.append((resident + swapped, purge))

    total = len(regions)
    n_mb = sum(fp for fp, p in regions if p == 'N') / 1024
    v_mb = sum(fp for fp, p in regions if p == 'V') / 1024
    return {'count': total, 'n_mb': n_mb, 'v_mb': v_mb}


def measure_and_report(pid: int, label: str):
    fp = measure_footprint(pid)
    g = measure_graphics(pid)
    log(
        f'{label:>16}  {fp:>10}  {g["count"]:>8}  '
        f'{g["n_mb"]:>8.0f}MB  {g["v_mb"]:>8.0f}MB'
    )


# ── 测试场景 ──────────────────────────────────────────────────────────────────

def phase_global_search():
    """全局搜索 — 覆盖 application / file / extension 即时答案 / web 类型。"""

    # application 类型
    for w in ['safa', 'term', 'note', 'code', 'musi', 'mail', 'calc', 'sett']:
        search_and_wait(w, SETTLE_INSTANT)

    # file / folder 类型
    for w in ['doc', 'pdf', 'config', 'desktop', 'download', ' readme', '.ts']:
        search_and_wait(w, SETTLE_DEFAULT)

    # extension 即时答案 — 计算器
    for expr in ['1+2', '3*4', '100-7', '2^10', 'sqrt(144)', 'sin(3.14)']:
        search_and_wait(expr, SETTLE_INSTANT)

    # extension 即时答案 — 货币换算（网络）
    for q in ['100 usd', '1 eur', '500 jpy']:
        search_and_wait(q, SETTLE_NETWORK)

    # extension 即时答案 — Base64 解码
    search_and_wait('SGVsbG8gV29ybGQ', SETTLE_INSTANT)

    # web 类型
    search_and_wait('//rust async', SETTLE_DEFAULT)
    clear_input()
    search_and_wait('//github.com', SETTLE_DEFAULT)
    clear_input()


def phase_tool_list():
    """工具列表模式（/ 前缀）。"""
    search_and_wait('/', SETTLE_INSTANT)
    press_down(3)
    press_up(2)

    for kw in ['/calc', '/clip', '/time', '/uuid', '/sett', '/base']:
        clear_input()
        type_text(kw)
        time.sleep(SETTLE_INSTANT)
        press_down(1)

    clear_input()


def phase_navigation():
    """结果键盘导航 + 进入/退出扩展。"""
    clear_input()
    type_text('a')
    time.sleep(SETTLE_DEFAULT)
    press_down(5)
    press_up(3)

    clear_input()
    type_text('s')
    time.sleep(SETTLE_DEFAULT)
    press_down(4)
    press_up(2)

    # 工具列表 → 进入第一个扩展 → 退出
    enter_ext('', SETTLE_DEFAULT)
    exit_ext()


def phase_enter_extensions():
    """进入扩展视图 — 覆盖全部含 mainView 的扩展 + 搜索型扩展的 DOM 渲染路径。"""

    # — 视图型扩展（mainView，渲染自定义 DOM）—

    enter_ext('clip', SETTLE_DEFAULT)       # 剪贴板列表
    press_down(3)
    exit_ext()

    enter_ext('sett', SETTLE_DEFAULT)       # 设置页
    press_down(3)
    exit_ext()

    enter_ext('uuid', SETTLE_INSTANT)       # UUID/NanoID 生成（disableSearchInput）
    press_down(2)
    exit_ext()

    enter_ext('system', SETTLE_DEFAULT)     # 系统状态（auto 高度，ResizeObserver）
    exit_ext()

    enter_ext('awake', SETTLE_INSTANT)      # 防休眠开关
    exit_ext()

    # screenshot：mainView 渲染设置页（快捷键配置 + 保存路径），不触发实际截屏
    enter_ext('screenshot', SETTLE_INSTANT)
    press_down(2)
    exit_ext()

    # window-manager：mainView 渲染设置页（启用开关 + 自定义尺寸），触发默认高度 480
    enter_ext('window', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # proxy：mainView 渲染代理视图，触发 840 高度（窗口高度动画）
    enter_ext('proxy', SETTLE_DEFAULT)
    press_down(3)
    exit_ext()

    # agent：mainView 渲染对话视图，触发 840 高度
    enter_ext('agent', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # translate：mainView 渲染翻译输入框，auto 高度（ResizeObserver）
    enter_ext('translate', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # image：mainView 渲染图片工具视图，auto 高度
    enter_ext('image', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # homebrew：mainView 渲染包列表
    enter_ext('brew', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # video：mainView 渲染视频工具，auto 高度
    enter_ext('video', SETTLE_DEFAULT)
    press_down(2)
    exit_ext()

    # clean-mode / ai-providers / finder-ext / zsh：设置型视图
    for ext_name in ['clean', 'provider', 'finder', 'zsh']:
        enter_ext(ext_name, SETTLE_INSTANT)
        press_down(2)
        exit_ext()

    # — 搜索型扩展（有 search，扩展内继续输入）—
    # 注意：不按 Enter 执行结果——copyAndHide 会延迟 800ms 隐藏窗口，
    # 时序难以精确控制，且隐藏后键盘焦点回到前台 app 导致后续输入泄漏。

    enter_ext('calc', SETTLE_INSTANT)
    type_text('2+3*4')
    time.sleep(SETTLE_INSTANT)
    press_down(1)
    clear_input()
    exit_ext()

    enter_ext('time', SETTLE_INSTANT)
    press_down(2)
    type_text('1700000000')
    time.sleep(SETTLE_INSTANT)
    clear_input()
    exit_ext()

    enter_ext('cur', SETTLE_NETWORK)
    type_text('100 eur')
    time.sleep(SETTLE_NETWORK)
    clear_input()
    exit_ext()

    enter_ext('ip', SETTLE_NETWORK)
    press_down(2)
    exit_ext()

    enter_ext('base', SETTLE_INSTANT)
    type_text('hello world')
    time.sleep(SETTLE_INSTANT)
    clear_input()
    exit_ext()


def phase_global_shortcuts():
    """全局快捷键触发的独立窗口 / 扩展激活路径。

    截屏（Alt+S）→ 截图 overlay 窗口（独立 WKWebView）→ Esc 退出
    扩展快捷键（Alt+C/T/A/F）→ 从隐藏唤起并激活扩展 → Esc 退出
    """
    global _win_visible, _in_ext

    # — 截屏快捷键：Alt+S 触发全屏截图 overlay（Rust hook 全权处理）—
    hide_window()
    trigger_ext_shortcut('s')
    time.sleep(SETTLE_DEFAULT)   # 等截图 overlay 渲染 + CGImage 编码
    press_esc()                  # 截图 overlay Esc → exit_impl 退出，主窗不自动恢复
    time.sleep(0.8)              # fade-out 动画 + focus 恢复
    _win_visible = False         # 主窗此时隐藏

    # — 扩展快捷键：从隐藏唤起 + 激活扩展 —
    for key in ['c', 't', 'a', 'f']:
        hide_window()
        trigger_ext_shortcut(key)
        time.sleep(TOGGLE_GAP + 0.3)
        _in_ext = True
        _win_visible = True
        press_down(2)
        exit_ext()
        hide_window()


def phase_snap_panel():
    """窗口管理 snap-panel：启用 → 鼠标触发面板 → 退出。

    snap-panel 是独立 HTML 入口（snap-panel.html），启用 WM 时懒创建 WebContent 进程。
    鼠标移至屏顶部中心触发区激活面板滑入，移开触发滑出。
    """
    global _in_ext

    # 进入 window-manager 扩展，Tab 到启用开关，Enter 切换
    show_window()
    enter_ext('window', SETTLE_DEFAULT)

    # Tab 到第一个 toggle（启用窗口管理），Enter 切换为开
    _post_key(KEY_TAB)
    time.sleep(0.2)
    press_enter()
    time.sleep(1.5)              # snap-panel 窗口懒创建 + WebContent 进程启动

    # 退出扩展回到主页（WM 已启用，snap-panel 窗口常驻）
    exit_ext()

    # 鼠标移至屏顶中心触发 snap-panel 滑入
    w, h = screen_size()
    move_mouse(w / 2, 2)        # CG 坐标原点在左上角，y=2 在 6px 触发区内
    time.sleep(0.8)             # 等面板滑入动画 + 渲染
    move_mouse(w / 2, h / 2)   # 移回中心，触发面板滑出
    time.sleep(0.6)

    # 再次触发（测试反复进出）
    move_mouse(w / 2, 2)
    time.sleep(0.5)
    move_mouse(w / 2, h / 2)
    time.sleep(0.5)

    # 禁用 WM：重新进入扩展，Tab 到 toggle，Enter 关闭
    enter_ext('window', SETTLE_DEFAULT)
    _post_key(KEY_TAB)
    time.sleep(0.2)
    press_enter()
    time.sleep(0.5)
    exit_ext()


# ── 主流程 ────────────────────────────────────────────────────────────────────

def run_test():
    global _win_visible

    pid = find_main_webcontent_pid()
    log(f'WebContent PID: {pid}')
    log(f'测试参数: {ROUNDS} 轮全场景{" (dev)" if DEV_MODE else ""}')
    log(f'每轮 = 全局搜索 + 工具列表 + 导航执行 + 扩展视图 + 全局快捷键 + snap-panel + hide/show')

    # 3 秒倒计时：让用户切走焦点，避免终端吃到后续按键
    log('\n3 秒后开始，请勿操作键盘鼠标...')
    for i in range(3, 0, -1):
        log(f'  {i}...')
        time.sleep(1)

    # 切到 ASCII 键盘布局（避免中文输入法拦截 CGEvent 按键）
    saved_input = switch_to_ascii()
    if saved_input:
        log('已切换到 ASCII 键盘布局')
    else:
        log('警告: 无法切换输入法，请手动切到英文')

    try:
        # 唤起 Voidnix（假设初始隐藏）
        log('\n唤起 Voidnix...')
        show_window()
        time.sleep(0.5)

        log(
            f'\n{"":>16}  {"FP":>10}  {"graphics":>8}  '
            f'{"PURGE=N":>10}  {"PURGE=V":>10}'
        )
        measure_and_report(pid, '基线')

        for r in range(1, ROUNDS + 1):
            log(f'\n── 第 {r}/{ROUNDS} 轮 ──────────────────────')

            show_window()
            phase_global_search()
            show_window()
            phase_tool_list()
            show_window()
            phase_navigation()
            show_window()
            phase_enter_extensions()

            measure_and_report(pid, f'第{r}轮 扩展视图')

            phase_global_shortcuts()
            show_window()
            phase_snap_panel()

            measure_and_report(pid, f'第{r}轮 完成')

            hide_window()
            time.sleep(0.5)
            show_window()
            time.sleep(0.3)
            measure_and_report(pid, f'第{r}轮 hide/show')

        hide_window()
        log('')
        measure_and_report(pid, '最终')
        log(f'\nhide 后回落 = 基线 vs 最终差值，反映非可回收层累积')
    finally:
        restore_input_source(saved_input)
        if saved_input:
            log('已恢复原始输入法')


if __name__ == '__main__':
    run_test()
