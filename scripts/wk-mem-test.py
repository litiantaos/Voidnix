#!/usr/bin/env python3
"""Voidnix WebKit 内存累积压测。

模拟真实使用模式覆盖全路径，分阶段测量主 WebContent 进程的 Physical footprint +
graphics 区域累积（PURGE=N 不可回收层）。

基础设施（CGEvent I/O / 窗口检测 / 内存测量）从 voidnix_test_lib 导入，
本文件仅含压测场景逻辑（phase_*）+ 内存趋势采样。

用法：
    python3 scripts/wk-mem-test.py              # 默认 5 轮全场景
    python3 scripts/wk-mem-test.py 10           # 10 轮
    python3 scripts/wk-mem-test.py 5 --dev      # dev 构建（快捷键叠加 Shift）

依赖：pyobjc-framework-Quartz（macOS 自带系统 Python 已含）
"""

import sys
import time

# 共享基础设施
from voidnix_test_lib import (
    log,
    # 时序
    SETTLE_INSTANT, SETTLE_DEFAULT, SETTLE_NETWORK, ESC_DELAY, TOGGLE_GAP,
    # 输入法
    switch_to_ascii, restore_input_source,
    # 窗口检测
    is_voidnix_visible, is_snap_panel_visible, snap_panel_exists,
    snap_panel_visible_bounds, voidnix_window_bounds,
    # 输入
    type_text, press_enter, press_esc, press_backspace, press_down, press_up,
    select_all, clear_input, search_and_wait, post_key,
    # 窗口操作
    show_window, hide_window, shortcut_press, trigger_ext_shortcut,
    # 鼠标
    click_at, move_mouse_to_snap_trigger, screen_size,
    # snap-panel
    trigger_snap_panel,
    # Finder
    ensure_finder_window, finder_window_bounds, close_finder_windows,
    # 内存
    WebContentTracker, measure_footprint, measure_graphics,
    # 常量
    KEY_TAB, KEY_ENTER,
)

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

# ── 扩展进出（_in_ext 状态封装）──────────────────────────────────────────────

_in_ext = False


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
    """退出扩展：Esc。若 _in_ext 失步则自动恢复。"""
    global _in_ext
    if not _in_ext:
        return
    press_esc()
    time.sleep(ESC_DELAY)
    _in_ext = False
    if not is_voidnix_visible():
        log('警告: exit_ext 后窗口隐藏，自动恢复...')
        show_window(DEV_MODE)


# ── 内存采样 ──────────────────────────────────────────────────────────────────

_tracker = WebContentTracker()


def measure_and_report(label: str):
    pid = _tracker.get()
    from voidnix_test_lib import measure_footprint_mb
    try:
        fp = measure_footprint(pid)
        g = measure_graphics(pid)
    except Exception:
        pid = _tracker.get(force=True)
        fp = measure_footprint(pid)
        g = measure_graphics(pid)
    log(
        f'{label:>16}  {fp:>10}  {g["count"]:>8}  '
        f'{g["n_mb"]:>8.0f}MB  {g["v_mb"]:>8.0f}MB'
    )


# ── 测试场景 ──────────────────────────────────────────────────────────────────

def phase_global_search():
    """全局搜索 — 覆盖 application / file / extension 即时答案 / web 类型。"""

    for w in ['safa', 'term', 'note', 'code', 'musi', 'mail', 'calc', 'sett']:
        search_and_wait(w, SETTLE_INSTANT)

    for w in ['doc', 'pdf', 'config', 'desktop', 'download', ' readme', '.ts']:
        search_and_wait(w, SETTLE_DEFAULT)

    for expr in ['1+2', '3*4', '100-7', '2^10', 'sqrt(144)', 'sin(3.14)']:
        search_and_wait(expr, SETTLE_INSTANT)

    for q in ['100 usd', '1 eur', '500 jpy']:
        search_and_wait(q, SETTLE_NETWORK)

    search_and_wait('SGVsbG8gV29ybGQ', SETTLE_INSTANT)

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

    enter_ext('', SETTLE_DEFAULT)
    exit_ext()


def phase_enter_extensions():
    """进入扩展视图 — 覆盖全部含 mainView 的扩展 + 搜索型扩展的 DOM 渲染路径。"""

    # 视图型扩展
    for ext_kw, settle, nav in [
        ('clip', SETTLE_DEFAULT, 3), ('sett', SETTLE_DEFAULT, 3),
        ('uuid', SETTLE_INSTANT, 2), ('system', SETTLE_DEFAULT, 0),
        ('awake', SETTLE_INSTANT, 0), ('screenshot', SETTLE_INSTANT, 2),
        ('window', SETTLE_DEFAULT, 2), ('proxy', SETTLE_DEFAULT, 3),
        ('agent', SETTLE_DEFAULT, 2), ('translate', SETTLE_DEFAULT, 2),
        ('image', SETTLE_DEFAULT, 2), ('brew', SETTLE_DEFAULT, 2),
        ('video', SETTLE_DEFAULT, 2),
    ]:
        enter_ext(ext_kw, settle)
        if nav:
            press_down(nav)
        exit_ext()

    for ext_name in ['clean', 'provider', 'finder', 'zsh']:
        enter_ext(ext_name, SETTLE_INSTANT)
        press_down(2)
        exit_ext()

    # 搜索型扩展
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
    """全局快捷键触发的独立窗口 / 扩展激活路径。"""
    global _in_ext

    # 截屏快捷键
    hide_window(DEV_MODE)
    trigger_ext_shortcut('s', DEV_MODE)
    time.sleep(SETTLE_DEFAULT)
    press_esc()
    time.sleep(0.8)

    # 扩展快捷键
    for key in ['c', 't', 'a', 'f']:
        hide_window(DEV_MODE)
        trigger_ext_shortcut(key, DEV_MODE)
        time.sleep(TOGGLE_GAP + 0.3)
        _in_ext = True
        press_down(2)
        exit_ext()
        hide_window(DEV_MODE)


def _wm_toggle():
    """在窗口管理扩展视图中按 Enter 切换启用开关（BaseList onExecute → toggle update）。"""
    from voidnix_test_lib import require_visible
    require_visible()
    press_enter()
    time.sleep(0.5)


def wait_snap_panel_gone(timeout=4.0, interval=0.3):
    """轮询等待 snap-panel 窗口从全量列表消失。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if not snap_panel_exists():
            return True
        time.sleep(interval)
    return False


def phase_snap_panel():
    """窗口管理 snap-panel 全链路验证 + 内存累积触发。"""
    global _in_ext

    w, h = screen_size()

    def _ensure_wm():
        hide_window(DEV_MODE)
        time.sleep(TOGGLE_GAP)
        if trigger_snap_panel(w, attempts=3):
            return True
        show_window(DEV_MODE)
        enter_ext('window', SETTLE_DEFAULT)
        _wm_toggle()
        time.sleep(1.5)
        exit_ext()
        hide_window(DEV_MODE)
        time.sleep(TOGGLE_GAP)
        return trigger_snap_panel(w, attempts=3)

    triggered = _ensure_wm()

    if snap_panel_exists():
        log('  [ok] snap-panel 窗口已创建')
    else:
        log('  [警告] snap-panel 窗口未创建')

    if triggered:
        log('  [ok] snap-panel 触发区显示正常')
    else:
        log('  [警告] snap-panel 触发失败')

    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.8)
    if not is_snap_panel_visible():
        log('  [ok] snap-panel 移出后隐藏正常')
    else:
        log('  [警告] snap-panel 移出触发区后未隐藏')

    if trigger_snap_panel(w, attempts=4):
        log('  [ok] snap-panel 二次触发正常')
    else:
        log('  [警告] snap-panel 二次触发未显示')

    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.7)

    # 布局点击验证
    ensure_finder_window()
    before = finder_window_bounds()

    if before:
        if trigger_snap_panel(w, attempts=5):
            time.sleep(0.4)
            panel = snap_panel_visible_bounds()
            if panel:
                click_x = panel[0] + 188
                click_y = panel[1] + 40
                click_at(click_x, click_y)
                time.sleep(1.5)

                panel_hid = not is_snap_panel_visible()
                after = finder_window_bounds()
                if after:
                    moved_right = after[0] > w * 0.4
                    narrowed = after[2] < w * 0.65
                    if moved_right and narrowed:
                        log(f'  [ok] 布局点击生效: Finder 移至右半屏')
                    elif panel_hid:
                        log(f'  [警告] 布局点击到达但 Finder 未移动')
                    else:
                        log('  [警告] 布局点击未生效: Finder frame 未变')
                else:
                    log('  [警告] 布局点击后无法读取 Finder bounds')
            else:
                log('  [警告] 布局点击阶段 snap-panel bounds 读取失败')
        else:
            log('  [警告] 布局点击阶段 snap-panel 未显示')
    else:
        log('  [警告] 无法打开 Finder 窗口')

    close_finder_windows()
    time.sleep(0.3)

    show_window(DEV_MODE)

    # 禁用 WM
    if snap_panel_exists():
        show_window(DEV_MODE)
        enter_ext('window', SETTLE_DEFAULT)
        _wm_toggle()
        exit_ext()

    if wait_snap_panel_gone():
        log('  [ok] snap-panel 窗口禁用后已销毁')
    else:
        log('  [警告] snap-panel 窗口禁用后仍存在')


# ── 主流程 ────────────────────────────────────────────────────────────────────

def run_test():
    pid = _tracker.get(force=True)
    log(f'WebContent PID: {pid}')
    log(f'测试参数: {ROUNDS} 轮全场景{" (dev)" if DEV_MODE else ""}')
    log(f'每轮 = 全局搜索 + 工具列表 + 导航执行 + 扩展视图 + 全局快捷键 + snap-panel + hide/show')

    log('\n3 秒后开始，请勿操作键盘鼠标...')
    for i in range(3, 0, -1):
        log(f'  {i}...')
        time.sleep(1)

    saved_input = switch_to_ascii()
    if saved_input:
        log('已切换到 ASCII 键盘布局')
    else:
        log('警告: 无法切换输入法，请手动切到英文')

    try:
        log('\n唤起 Voidnix...')
        show_window(DEV_MODE)
        time.sleep(0.5)

        log(
            f'\n{"":>16}  {"FP":>10}  {"graphics":>8}  '
            f'{"PURGE=N":>10}  {"PURGE=V":>10}'
        )
        measure_and_report('基线')

        for r in range(1, ROUNDS + 1):
            log(f'\n── 第 {r}/{ROUNDS} 轮 ──────────────────────')

            show_window(DEV_MODE)
            phase_global_search()
            show_window(DEV_MODE)
            phase_tool_list()
            show_window(DEV_MODE)
            phase_navigation()
            show_window(DEV_MODE)
            phase_enter_extensions()

            measure_and_report(f'第{r}轮 扩展视图')

            phase_global_shortcuts()
            show_window(DEV_MODE)
            phase_snap_panel()

            measure_and_report(f'第{r}轮 完成')

            hide_window(DEV_MODE)
            time.sleep(0.5)
            show_window(DEV_MODE)
            time.sleep(0.3)
            measure_and_report(f'第{r}轮 hide/show')

        hide_window(DEV_MODE)
        log('')
        measure_and_report('最终')
        log(f'\nhide 后回落 = 基线 vs 最终差值，反映非可回收层累积')
    finally:
        restore_input_source(saved_input)
        if saved_input:
            log('已恢复原始输入法')


if __name__ == '__main__':
    run_test()
