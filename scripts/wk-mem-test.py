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


# ── 窗口可见性真实检测 ──────────────────────────────────────────────────────────
# Voidnix 是 Accessory app（不 activate），窗口隐藏后键盘焦点回到前台 app。
# CGEvent 是全局 HID 事件，发到当前 key window——如果 Voidnix 窗口不可见，
# 按键会打到终端 / IDE。
#
# Voidnix hide 策略 = alpha=0 + ignoresMouse（不 orderOut），窗口仍在窗口服务器
# 列表中但 kCGWindowAlpha=0。用此区分真实可见 / 隐藏，不依赖启发式追踪。

_in_ext = False        # 是否在扩展视图内（决定 Esc 语义）


def log(msg: str):
    print(msg, flush=True)


def is_voidnix_visible() -> bool:
    """检测 Voidnix 主窗口是否真正可见（alpha > 0 且有尺寸）。"""
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        alpha = w.get('kCGWindowAlpha', 0)
        bounds = w.get('kCGWindowBounds', {})
        if alpha > 0.01 and bounds.get('Width', 0) > 300 and bounds.get('Height', 0) > 200:
            return True
    return False


def _require_visible():
    """按键前安全守卫：窗口不可见时立即中止，防注入到其他应用。"""
    if not is_voidnix_visible():
        raise RuntimeError(
            'Voidnix 窗口意外隐藏，中止测试以防按键泄漏到其他应用。'
            '常见原因：blur 失焦隐藏、_in_ext 失步致 Esc 在全局模式触发隐藏、'
            'copyAndHide 延迟隐藏。'
        )


# ── snap-panel 窗口检测 ───────────────────────────────────────────────────────
# snap-panel 尺寸（与 window_snap.rs panel_dimensions 同步）：
#   单屏 352×80 / 多屏 420×80；主窗口固定 720×480。
# snap-panel 创建初始 600×300（WebviewWindowBuilder inner_size），show 后变为面板尺寸。
# 隐藏策略同主窗口：alpha=0 + ignoresMouse（不 orderOut），窗口仍在全量列表中。

_SNAP_W_MIN = 300
_SNAP_W_MAX = 460
_SNAP_H_MIN = 65
_SNAP_H_MAX = 100


def snap_panel_exists() -> bool:
    """检测 snap-panel 窗口是否已创建（含 alpha=0 不可见状态）。

    在全量窗口列表中查找 Voidnix 拥有、非主窗口尺寸的窗口。
    """
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionAll, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        bounds = w.get('kCGWindowBounds', {})
        bw = bounds.get('Width', 0)
        bh = bounds.get('Height', 0)
        # 排除主窗口（720×480）与过小条目（status item 等）
        if bw > 200 and bh > 50 and not (bw > 680 and bh > 400):
            return True
    return False


def is_snap_panel_visible() -> bool:
    """检测 snap-panel 是否可见（alpha > 0 且尺寸匹配面板）。"""
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        alpha = w.get('kCGWindowAlpha', 0)
        bounds = w.get('kCGWindowBounds', {})
        bw = bounds.get('Width', 0)
        bh = bounds.get('Height', 0)
        if (alpha > 0.5 and _SNAP_W_MIN <= bw <= _SNAP_W_MAX
                and _SNAP_H_MIN <= bh <= _SNAP_H_MAX):
            return True
    return False


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
    """确保窗口可见。基于真实可见性检测，不依赖启发式。"""
    if is_voidnix_visible():
        return
    _shortcut_press()
    time.sleep(TOGGLE_GAP)
    if not is_voidnix_visible():
        raise RuntimeError('show_window: 快捷键未能显示窗口，可能被其他应用抢占焦点')


def hide_window():
    """确保窗口隐藏。仅在真正可见时 toggle。"""
    if not is_voidnix_visible():
        return
    _shortcut_press()
    time.sleep(TOGGLE_GAP)


# ── 输入操作（仅在窗口可见时调用）────────────────────────────────────────────

def type_text(s: str, delay: float = TYPE_DELAY):
    _require_visible()
    for ch in s:
        kc = _char_to_keycode(ch)
        if kc:
            _post_key(kc[0], flags=kc[1])
            time.sleep(delay)


def press_enter():
    _require_visible()
    _post_key(KEY_ENTER)


def press_esc():
    _post_key(KEY_ESC)


def press_backspace():
    _require_visible()
    _post_key(KEY_BACKSPACE)


def press_down(n: int = 1, delay: float = NAV_DELAY):
    _require_visible()
    for _ in range(n):
        _post_key(KEY_DOWN)
        time.sleep(delay)


def press_up(n: int = 1, delay: float = NAV_DELAY):
    _require_visible()
    for _ in range(n):
        _post_key(KEY_UP)
        time.sleep(delay)


def select_all():
    _require_visible()
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
    """退出扩展：Esc。若 _in_ext 失步（实际在全局模式），Esc 会隐藏窗口——
    检测到隐藏则自动 show_window 恢复到全局模式，并输出警告。"""
    global _in_ext
    if not _in_ext:
        return
    press_esc()
    time.sleep(ESC_DELAY)
    _in_ext = False
    if not is_voidnix_visible():
        log('警告: exit_ext 后窗口隐藏（_in_ext 失步，Esc 在全局模式触发了 hide），自动恢复...')
        show_window()


# ── 鼠标操作 ──────────────────────────────────────────────────────────────────

def _voidnix_pid():
    """获取 Voidnix 主进程 PID。"""
    try:
        out = subprocess.check_output(
            ['pgrep', '-f', 'Voidnix.app/Contents/MacOS/Voidnix'], text=True
        )
        return int(out.strip().splitlines()[0])
    except (subprocess.CalledProcessError, ValueError, IndexError):
        return None


def move_mouse_to_snap_trigger(x: float, y: float):
    """专用：移动鼠标到 snap-panel 触发区，确保 drag monitor 收到事件。

    global monitor 对合成 mouseMoved 的捕获是非确定性的。三路投递最大化命中率：
      1. CGWarpMouseCursorPosition 移动光标（更新 NSEvent.mouseLocation）
      2. CGEventPostToPid 直接送 Voidnix local monitor
      3. CGEventPost(kCGHIDEventTap) 走系统分发，global monitor 可捕获
    调用方应交替 y 坐标——同坐标重复 Warp 不生成新事件，monitor 不触发。
    """
    point = Quartz.CGPoint(x, y)
    Quartz.CGWarpMouseCursorPosition(point)
    time.sleep(0.05)
    pid = _voidnix_pid()
    # 双路投递：local monitor (PostToPid) + global monitor (HIDEventTap)
    ev_local = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, point, 0)
    if pid:
        Quartz.CGEventPostToPid(pid, ev_local)
    ev_hid = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, point, 0)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, ev_hid)
    time.sleep(0.05)


def click_at(x: float, y: float):
    """在 (x, y) 处模拟左键单击（CGEvent 坐标，top-left 原点）。

    先 Warp 光标到目标位置（确保窗口服务器 hit-test 到正确窗口），再发 mouseDown/Up。
    不 Warp 时合成事件可能投递到 Warp 前的旧光标位置对应的窗口。
    """
    point = Quartz.CGPoint(x, y)
    Quartz.CGWarpMouseCursorPosition(point)
    time.sleep(0.03)
    down = Quartz.CGEventCreateMouseEvent(
        None, Quartz.kCGEventLeftMouseDown, point, Quartz.kCGMouseButtonLeft
    )
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, down)
    time.sleep(0.08)
    up = Quartz.CGEventCreateMouseEvent(
        None, Quartz.kCGEventLeftMouseUp, point, Quartz.kCGMouseButtonLeft
    )
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)


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


# hide_window 后 Rust 端内存阈值重载（footprint > 350M → about:blank → reload）
# 会替换 WebContent 进程，PID 变化。缓存失效时重新查找。
_wc_pid = None


def _pid_alive(pid: int) -> bool:
    return subprocess.run(
        ['kill', '-0', str(pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    ).returncode == 0


def wc_pid(force: bool = False) -> int:
    global _wc_pid
    if not force and _wc_pid and _pid_alive(_wc_pid):
        return _wc_pid
    _wc_pid = find_main_webcontent_pid()
    return _wc_pid


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


def measure_and_report(label: str):
    pid = wc_pid()
    try:
        fp = measure_footprint(pid)
        g = measure_graphics(pid)
    except subprocess.CalledProcessError:
        # WebContent 进程被内存阈值重载替换，重新解析 PID 重试
        pid = wc_pid(force=True)
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
    global _in_ext

    # — 截屏快捷键：Alt+S 触发全屏截图 overlay（Rust hook 全权处理）—
    hide_window()
    trigger_ext_shortcut('s')
    time.sleep(SETTLE_DEFAULT)   # 等截图 overlay 渲染 + CGImage 编码
    press_esc()                  # 截图 overlay Esc → exit_impl 退出，主窗不自动恢复
    time.sleep(0.8)              # fade-out 动画 + focus 恢复
    # 主窗此时隐藏（screenshot overlay Esc 不恢复主窗）

    # — 扩展快捷键：从隐藏唤起 + 激活扩展 —
    for key in ['c', 't', 'a', 'f']:
        hide_window()
        trigger_ext_shortcut(key)
        time.sleep(TOGGLE_GAP + 0.3)
        _in_ext = True
        press_down(2)
        exit_ext()
        hide_window()


def _wm_toggle():
    """在 window-manager 扩展视图中 Tab 到启用开关并 Enter 切换。"""
    _require_visible()
    _post_key(KEY_TAB)
    time.sleep(0.2)
    press_enter()


def wait_snap_panel_gone(timeout=4.0, interval=0.3):
    """轮询等待 snap-panel 窗口从全量列表消失（close 异步，固定 sleep 不可靠）。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if not snap_panel_exists():
            return True
        time.sleep(interval)
    return False


def snap_panel_visible_bounds():
    """读取可见 snap-panel 的 bounds（CGWindowList，top-left 原点）。

    供布局点击计算 zone 在屏幕上的绝对坐标。返回 (x, y, w, h) 或 None。
    """
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        alpha = w.get('kCGWindowAlpha', 0)
        bounds = w.get('kCGWindowBounds', {})
        bw = bounds.get('Width', 0)
        bh = bounds.get('Height', 0)
        if (alpha > 0.5 and _SNAP_W_MIN <= bw <= _SNAP_W_MAX
                and _SNAP_H_MIN <= bh <= _SNAP_H_MAX):
            return (bounds.get('X', 0), bounds.get('Y', 0), bw, bh)
    return None


# ── 布局目标窗口管理 ──────────────────────────────────────────────────────────
# 用 Finder 窗口作为可控布局目标：打开 → 激活 → snap-panel 的 capture_frontmost 记录其
# PID → 点击 zone → AX 写入其 frame → 读取 bounds 验证确实变化。

def ensure_finder_window():
    """关闭所有 Finder 窗口后新建一个，确保只有一个窗口作为布局目标。

    多窗口时 finder_window_bounds() 读的 CGWindowList 首个窗口可能与 AX 布局作用的
    AXFocusedWindow 不是同一个，导致验证读错窗口。关闭全部再建一个消除歧义。
    """
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to close every window'],
        capture_output=True,
    )
    time.sleep(0.3)
    subprocess.Popen(['open', '-a', 'Finder'])
    time.sleep(0.5)
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to make new Finder window'],
        capture_output=True,
    )
    time.sleep(0.5)
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to activate'],
        capture_output=True,
    )
    time.sleep(0.6)


def _finder_pid():
    """获取 Finder 进程 PID（pgrep 按进程名匹配，不受窗口 owner 本地化名影响）。"""
    try:
        out = subprocess.check_output(['pgrep', '-x', 'Finder'], text=True)
        return int(out.strip().splitlines()[0])
    except (subprocess.CalledProcessError, ValueError, IndexError):
        return None


def finder_window_bounds():
    """读取 Finder 主窗口 bounds（CGWindowList，top-left 原点）。

    按 PID 匹配而非 owner name——窗口 owner 名随系统语言本地化（中文=「访达」）。
    CGWindowList 的 bounds 坐标原点在主屏左上角（与 CGEvent / AX 同系），
    直接与布局算法的 AX 坐标对比。返回 (x, y, w, h) 或 None。
    """
    pid = _finder_pid()
    if not pid:
        return None
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerPID') != pid:
            continue
        if w.get('kCGWindowLayer', 1) != 0:
            continue
        bounds = w.get('kCGWindowBounds', {})
        bw = bounds.get('Width', 0)
        bh = bounds.get('Height', 0)
        if bw > 200 and bh > 200:
            return (bounds.get('X', 0), bounds.get('Y', 0), bw, bh)
    return None


def close_finder_windows():
    """关闭所有 Finder 窗口（测试清理）。"""
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to close every window'],
        capture_output=True,
    )


def trigger_snap_panel(screen_w: float, attempts: int = 5, interval: float = 0.35) -> bool:
    """触发 snap-panel 显示，带重试克服 CGEvent 合成事件对 NSEvent monitor 的非确定性捕获。

    策略：每次尝试先从触发区外（y=100）移入触发区（y=1/3/5 交替），模拟真实鼠标进入。
    同坐标重复 Warp 不生成新事件；交替坐标 + 从外移入强制产生 distinct mouseMoved 事件流。
    """
    for i in range(attempts):
        # 先移到触发区下方（y=100，明确在 zone 外）
        move_mouse_to_snap_trigger(screen_w / 2, 100)
        time.sleep(0.08)
        # 再移入触发区（交替 y=1/3/5，均在 6px zone 内）
        y = 1 + (i % 3) * 2
        move_mouse_to_snap_trigger(screen_w / 2, y)
        time.sleep(interval)
        if is_snap_panel_visible():
            return True
    return False


def phase_snap_panel():
    """窗口管理 snap-panel 全链路验证 + 内存累积触发。

    snap-panel 是独立 HTML 入口（snap-panel.html），启用 WM 时懒创建 WebContent 进程。
    验证链：确保启用 → 窗口创建 → 鼠标触发可见 → 移出隐藏 → 二次触发 → 布局点击 → 禁用 → 销毁。
    """
    global _in_ext

    w, h = screen_size()

    # —— 确保 WM 启用：用面板触发作为状态真相 ——
    # snap_panel_exists() 不可靠（close bug 致窗口残留），Tab+Enter 方向歧义。
    # 直接试触发：成功=已启用，失败=toggle 再试。trigger_snap_panel 带重试，
    # 若 monitor 在运行终会成功；若 monitor 已停再多重试也无用——是可靠的状态判定。
    def _ensure_wm():
        hide_window()
        time.sleep(TOGGLE_GAP)
        if trigger_snap_panel(w, attempts=3):
            return True
        # 未启用或 monitor 已停——toggle 一次再试
        show_window()
        enter_ext('window', SETTLE_DEFAULT)
        _wm_toggle()
        time.sleep(1.5)
        exit_ext()
        hide_window()
        time.sleep(TOGGLE_GAP)
        return trigger_snap_panel(w, attempts=3)

    triggered = _ensure_wm()

    if snap_panel_exists():
        log('  [ok] snap-panel 窗口已创建')
    else:
        log('  [警告] snap-panel 窗口未创建——窗口管理启用可能失败')

    if triggered:
        log('  [ok] snap-panel 触发区显示正常')
    else:
        log('  [警告] snap-panel 触发失败——drag monitor 未注册或 CGEvent 限制')

    # 移回中心，等面板滑出（hide timer 0.4s + 淡出动画 0.2s）
    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.8)
    if not is_snap_panel_visible():
        log('  [ok] snap-panel 移出后隐藏正常')
    else:
        log('  [警告] snap-panel 移出触发区后未隐藏')

    # 二次触发（测试反复进出稳定性 + 内存累积）
    if trigger_snap_panel(w, attempts=4):
        log('  [ok] snap-panel 二次触发正常')
    else:
        log('  [警告] snap-panel 二次触发未显示')

    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.7)

    # —— 布局点击验证：完整链路（触发面板 → 点击 zone → 前台窗口 frame 变化）——
    ensure_finder_window()
    before = finder_window_bounds()

    if before:
        if trigger_snap_panel(w, attempts=5):
            time.sleep(0.4)       # 等动画完成 + DOM 就绪
            panel = snap_panel_visible_bounds()
            if panel:
                # SnapPanel.vue zone 布局（单屏 5 组，panel 352×80）：
                #   根 p-3(12) | g0 quarters 56 | gap 12 | g1 halves-v 56 | gap 12
                #   | g2 halves-h 56 | gap 12 | g3 full-center 56 | gap 12 | g4 custom 56 | p-3
                # g2 halves-h 右列中心：local x≈188, y=40（panel 纵向中点）
                click_x = panel[0] + 188
                click_y = panel[1] + 40
                click_at(click_x, click_y)
                time.sleep(1.5)   # Vue @click → invoke → Rust AX 写入

                # do_set_layout 成功后调 hide_panel——面板隐藏说明点击到达 + 命令执行
                panel_hid = not is_snap_panel_visible()

                after = finder_window_bounds()
                if after:
                    moved_right = after[0] > w * 0.4
                    narrowed = after[2] < w * 0.65
                    if moved_right and narrowed:
                        log(f'  [ok] 布局点击生效: Finder 移至右半屏 '
                            f'({before[2]:.0f}×{before[3]:.0f}'
                            f' → {after[0]:.0f},{after[2]:.0f}×{after[3]:.0f})')
                    elif panel_hid:
                        log(f'  [警告] 布局点击到达（面板已隐藏）但 Finder 未移动 '
                            f'(before={before[0]:.0f},{before[2]:.0f}×{before[3]:.0f}'
                            f' after={after[0]:.0f},{after[2]:.0f}×{after[3]:.0f})'
                            f'——AX 写入失败或 Finder 不可缩放')
                    else:
                        log('  [警告] 布局点击未生效: Finder frame 未变')
                else:
                    log('  [警告] 布局点击后无法读取 Finder 窗口 bounds')
            else:
                log('  [警告] 布局点击阶段 snap-panel bounds 读取失败')
        else:
            log('  [警告] 布局点击阶段 snap-panel 未显示，跳过点击验证')
    else:
        log('  [警告] 无法打开 Finder 窗口作为布局目标，跳过点击验证')

    # 清理 Finder 窗口
    close_finder_windows()
    time.sleep(0.3)

    # 恢复主窗口可见（后续键盘操作需要）
    show_window()

    # 禁用 WM
    if snap_panel_exists():
        show_window()
        enter_ext('window', SETTLE_DEFAULT)
        _wm_toggle()
        exit_ext()

    if wait_snap_panel_gone():
        log('  [ok] snap-panel 窗口禁用后已销毁')
    else:
        log('  [警告] snap-panel 窗口禁用后仍存在——getAllWebviewWindows().close() 可能未销毁窗口')


# ── 主流程 ────────────────────────────────────────────────────────────────────

def run_test():
    pid = wc_pid(force=True)
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
        measure_and_report('基线')

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

            measure_and_report(f'第{r}轮 扩展视图')

            phase_global_shortcuts()
            show_window()
            phase_snap_panel()

            measure_and_report(f'第{r}轮 完成')

            hide_window()
            time.sleep(0.5)
            show_window()
            time.sleep(0.3)
            measure_and_report(f'第{r}轮 hide/show')

        hide_window()
        log('')
        measure_and_report('最终')
        log(f'\nhide 后回落 = 基线 vs 最终差值，反映非可回收层累积')
    finally:
        restore_input_source(saved_input)
        if saved_input:
            log('已恢复原始输入法')


if __name__ == '__main__':
    run_test()
