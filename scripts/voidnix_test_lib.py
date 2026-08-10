"""Voidnix 测试共享基础设施。

wk-mem-test.py（内存压测）与 smoke-test.py（全功能回归）共用的 CGEvent I/O、
窗口检测、内存测量等基础设施。纯工具函数，不含测试逻辑。

设计：所有函数接受 dev_mode 参数（而非读全局），调用方在启动时确定模式后传入。
_in_ext 状态由 TestContext 类封装，避免模块级可变全局。
"""

import ctypes
import re
import subprocess
import time

import Quartz

# ── 时序常量 ──────────────────────────────────────────────────────────────────

TYPE_DELAY = 0.045
SETTLE_INSTANT = 0.6
SETTLE_DEFAULT = 1.0
SETTLE_NETWORK = 2.5
NAV_DELAY = 0.12
ESC_DELAY = 0.3
TOGGLE_GAP = 1.0

# ── snap-panel 尺寸（与 window_snap.rs panel_dimensions 同步）─────────────────

_SNAP_W_MIN = 300
_SNAP_W_MAX = 460
_SNAP_H_MIN = 65
_SNAP_H_MAX = 100

# ── 修饰符 ────────────────────────────────────────────────────────────────────

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

# ── 输入法管理（HIToolbox TIS API）────────────────────────────────────────────

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


# ── 窗口可见性检测 ────────────────────────────────────────────────────────────

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


def require_visible():
    """按键前安全守卫：窗口不可见时立即中止，防注入到其他应用。"""
    if not is_voidnix_visible():
        raise RuntimeError(
            'Voidnix 窗口意外隐藏，中止测试以防按键泄漏到其他应用。'
        )


def voidnix_window_bounds():
    """返回主窗口 bounds (x, y, w, h)，不可见时返回 None。"""
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        alpha = w.get('kCGWindowAlpha', 0)
        bounds = w.get('kCGWindowBounds', {})
        if alpha > 0.01 and bounds.get('Width', 0) > 300 and bounds.get('Height', 0) > 200:
            return (bounds.get('X', 0), bounds.get('Y', 0), bounds.get('Width', 0), bounds.get('Height', 0))
    return None


def count_voidnix_windows() -> int:
    """统计屏幕上可见的 Voidnix 窗口数（含 overlay / snap-panel）。"""
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionOnScreenOnly, Quartz.kCGNullWindowID
    )
    count = 0
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        if w.get('kCGWindowAlpha', 0) > 0.01:
            bounds = w.get('kCGWindowBounds', {})
            if bounds.get('Width', 0) > 50 and bounds.get('Height', 0) > 50:
                count += 1
    return count


# ── snap-panel 检测 ───────────────────────────────────────────────────────────

def snap_panel_exists() -> bool:
    """检测 snap-panel 窗口是否已创建（含 alpha=0 不可见状态）。"""
    wl = Quartz.CGWindowListCopyWindowInfo(
        Quartz.kCGWindowListOptionAll, Quartz.kCGNullWindowID
    )
    for w in wl:
        if w.get('kCGWindowOwnerName', '') != 'Voidnix':
            continue
        bounds = w.get('kCGWindowBounds', {})
        bw = bounds.get('Width', 0)
        bh = bounds.get('Height', 0)
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


def snap_panel_visible_bounds():
    """读取可见 snap-panel 的 bounds (x, y, w, h)，不可见返回 None。"""
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


# ── 键盘输入 ──────────────────────────────────────────────────────────────────

def _char_to_keycode(ch: str):
    if ch in _BASE:
        return (_BASE[ch], 0)
    lc = ch.lower()
    if ch.isupper() and lc in _BASE:
        return (_BASE[lc], SHIFT)
    if ch in _SHIFTED:
        return (_SHIFTED[ch], SHIFT)
    return None


def post_key(keycode: int, flags: int = 0):
    down = Quartz.CGEventCreateKeyboardEvent(None, keycode, True)
    Quartz.CGEventSetFlags(down, flags)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, down)
    up = Quartz.CGEventCreateKeyboardEvent(None, keycode, False)
    Quartz.CGEventSetFlags(up, flags)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, up)


def ext_flags(dev_mode: bool) -> int:
    """扩展快捷键修饰符（Alt 基，dev 叠加 Shift）。"""
    flags = ALT
    if dev_mode:
        flags |= SHIFT
    return flags


def shortcut_press(dev_mode: bool):
    """Option+Space（dev 叠加 Shift）。"""
    post_key(49, flags=ext_flags(dev_mode))


def trigger_ext_shortcut(key_char: str, dev_mode: bool):
    """触发扩展全局快捷键（Alt+key，dev 叠加 Shift）。"""
    kc = _char_to_keycode(key_char.lower())
    if kc:
        post_key(kc[0], flags=ext_flags(dev_mode))


# ── 窗口操作 ──────────────────────────────────────────────────────────────────

def show_window(dev_mode: bool):
    """确保窗口可见。基于真实可见性检测，不依赖启发式。"""
    if is_voidnix_visible():
        return
    shortcut_press(dev_mode)
    time.sleep(TOGGLE_GAP)
    if not is_voidnix_visible():
        raise RuntimeError('show_window: 快捷键未能显示窗口，可能被其他应用抢占焦点')


def hide_window(dev_mode: bool):
    """确保窗口隐藏。仅在真正可见时 toggle。"""
    if not is_voidnix_visible():
        return
    shortcut_press(dev_mode)
    time.sleep(TOGGLE_GAP)


# ── 文本输入 ──────────────────────────────────────────────────────────────────

def type_text(s: str, delay: float = TYPE_DELAY):
    require_visible()
    for ch in s:
        kc = _char_to_keycode(ch)
        if kc:
            post_key(kc[0], flags=kc[1])
            time.sleep(delay)


def press_enter():
    require_visible()
    post_key(KEY_ENTER)


def press_esc():
    post_key(KEY_ESC)


def press_backspace():
    require_visible()
    post_key(KEY_BACKSPACE)


def press_down(n: int = 1, delay: float = NAV_DELAY):
    require_visible()
    for _ in range(n):
        post_key(KEY_DOWN)
        time.sleep(delay)


def press_up(n: int = 1, delay: float = NAV_DELAY):
    require_visible()
    for _ in range(n):
        post_key(KEY_UP)
        time.sleep(delay)


def select_all():
    require_visible()
    post_key(_BASE['a'], flags=CMD)


def clear_input():
    """Cmd+A → Backspace 清空搜索框。"""
    select_all()
    time.sleep(0.04)
    press_backspace()
    time.sleep(0.08)


def search_and_wait(query: str, settle: float = SETTLE_DEFAULT):
    """清空 → 输入 → 等待结果渲染。"""
    clear_input()
    type_text(query)
    time.sleep(settle)


# ── 鼠标操作 ──────────────────────────────────────────────────────────────────

def voidnix_pid():
    """获取 Voidnix 主进程 PID。"""
    try:
        out = subprocess.check_output(
            ['pgrep', '-f', 'Voidnix.app/Contents/MacOS/Voidnix'], text=True
        )
        return int(out.strip().splitlines()[0])
    except (subprocess.CalledProcessError, ValueError, IndexError):
        return None


def move_mouse_to_snap_trigger(x: float, y: float):
    """专用：移动鼠标到 snap-panel 触发区，三路投递最大化命中率。"""
    point = Quartz.CGPoint(x, y)
    Quartz.CGWarpMouseCursorPosition(point)
    time.sleep(0.05)
    pid = voidnix_pid()
    ev_local = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, point, 0)
    if pid:
        Quartz.CGEventPostToPid(pid, ev_local)
    ev_hid = Quartz.CGEventCreateMouseEvent(None, Quartz.kCGEventMouseMoved, point, 0)
    Quartz.CGEventPost(Quartz.kCGHIDEventTap, ev_hid)
    time.sleep(0.05)


def click_at(x: float, y: float):
    """在 (x, y) 处模拟左键单击。"""
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


class WebContentTracker:
    """跟踪 WebContent PID，处理内存阈值重载后的 PID 变化。"""

    def __init__(self):
        self._pid = None

    def get(self, force: bool = False) -> int:
        if not force and self._pid and self._is_alive(self._pid):
            return self._pid
        self._pid = find_main_webcontent_pid()
        return self._pid

    @staticmethod
    def _is_alive(pid: int) -> bool:
        return subprocess.run(
            ['kill', '-0', str(pid)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        ).returncode == 0


def measure_footprint(pid: int) -> str:
    out = subprocess.check_output(
        ['vmmap', '--summary', str(pid)], text=True, stderr=subprocess.DEVNULL
    )
    for line in out.splitlines():
        if 'Physical footprint:' in line and 'peak' not in line:
            return line.strip().split(':')[1].strip()
    return '?'


def measure_footprint_mb(pid: int) -> float:
    """返回 Physical footprint（MB），解析失败返回 -1。"""
    fp = measure_footprint(pid)
    return parse_footprint_mb(fp)


def parse_footprint_mb(fp_str: str) -> float:
    """将 footprint 字符串（如 "187.3M"）解析为 MB float。"""
    m = re.match(r'([\d.]+)\s*([KMGT]?)', fp_str)
    if not m:
        return -1
    val = float(m.group(1))
    unit = m.group(2)
    if unit == 'K':
        return val / 1024
    if unit == 'M':
        return val
    if unit == 'G':
        return val * 1024
    if unit == 'T':
        return val * 1024 * 1024
    return val


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


def measure_memory(tracker: WebContentTracker) -> dict:
    """一次性采集 footprint + graphics，处理 PID 失效重试。"""
    pid = tracker.get()
    try:
        return {
            'footprint_mb': measure_footprint_mb(pid),
            'graphics': measure_graphics(pid),
        }
    except subprocess.CalledProcessError:
        pid = tracker.get(force=True)
        return {
            'footprint_mb': measure_footprint_mb(pid),
            'graphics': measure_graphics(pid),
        }


# ── Finder 辅助（snap-panel 布局验证目标窗口）─────────────────────────────────

def finder_pid():
    """获取 Finder 进程 PID。"""
    try:
        out = subprocess.check_output(['pgrep', '-x', 'Finder'], text=True)
        return int(out.strip().splitlines()[0])
    except (subprocess.CalledProcessError, ValueError, IndexError):
        return None


def finder_window_bounds():
    """读取 Finder 主窗口 bounds (x, y, w, h)，无窗口返回 None。"""
    pid = finder_pid()
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


def ensure_finder_window():
    """关闭所有 Finder 窗口后新建一个，确保只有一个窗口作为布局目标。

    仅用 AppleScript 创建（open -a Finder 在无窗口时会自动弹窗，
    与 make new Finder window 叠加创建两个）。
    """
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to close every window'],
        capture_output=True,
    )
    time.sleep(0.3)
    subprocess.run(
        [
            'osascript',
            '-e',
            'tell application "Finder" to make new Finder window',
            '-e',
            'tell application "Finder" to activate',
        ],
        capture_output=True,
    )
    time.sleep(1.1)


def close_finder_windows():
    """关闭所有 Finder 窗口（测试清理）。"""
    subprocess.run(
        ['osascript', '-e', 'tell application "Finder" to close every window'],
        capture_output=True,
    )


# ── 进程管理 ──────────────────────────────────────────────────────────────────

def kill_voidnix():
    """终止运行中的 Voidnix 实例。"""
    subprocess.run(['pkill', '-x', 'Voidnix'], capture_output=True)
    time.sleep(1)


def trigger_snap_panel(screen_w: float, attempts: int = 5, interval: float = 0.35) -> bool:
    """触发 snap-panel 显示，带重试克服 CGEvent 非确定性捕获。"""
    for i in range(attempts):
        move_mouse_to_snap_trigger(screen_w / 2, 100)
        time.sleep(0.08)
        y = 1 + (i % 3) * 2
        move_mouse_to_snap_trigger(screen_w / 2, y)
        time.sleep(interval)
        if is_snap_panel_visible():
            return True
    return False
