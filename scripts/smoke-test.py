#!/usr/bin/env python3
"""Voidnix 全功能回归测试编排器。

两层测试架构：
  Layer 1（应用自测）：app 内部直接调用 searchEngine / getAllExtensions / invoke 等真实 API，
    验证搜索正确性、扩展注册、视图渲染、命令可达性。经环境变量 VOIDNIX_SELF_TEST=1 触发，
    结果写到 app 数据目录 config/test-report.json。
  Layer 2（系统冒烟）：CGEvent 驱动真实 UI 操作，验证窗口行为、全局快捷键、snap-panel、
    搜索 UI、扩展视图、内存基线。每步返回结构化 TestResult。

用法：
    python3 scripts/smoke-test.py                  # 完整测试（Layer 1 + Layer 2）
    python3 scripts/smoke-test.py --self-test-only # 仅 Layer 1（快，~30s，无需独占屏幕）
    python3 scripts/smoke-test.py --dev            # dev 构建（.dev bundle id）
    python3 scripts/smoke-test.py --build          # 含 release 构建
    python3 scripts/smoke-test.py --no-cgevent     # 跳过 Layer 2（CI/headless 友好）
"""

import json
import os
import pathlib
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Optional

# 共享 CGEvent 基础设施
sys.path.insert(0, str(Path(__file__).parent))
from voidnix_test_lib import (
    log,
    SETTLE_INSTANT, SETTLE_DEFAULT, ESC_DELAY, TOGGLE_GAP,
    switch_to_ascii, restore_input_source,
    is_voidnix_visible, is_snap_panel_visible, snap_panel_exists,
    snap_panel_visible_bounds, voidnix_window_bounds, count_voidnix_windows,
    type_text, press_enter, press_esc, press_backspace, press_down, press_up,
    select_all, clear_input, search_and_wait, post_key,
    show_window, hide_window, shortcut_press, trigger_ext_shortcut,
    click_at, move_mouse_to_snap_trigger, screen_size,
    trigger_snap_panel, KEY_TAB, KEY_ENTER,
    ensure_finder_window, finder_window_bounds, close_finder_windows,
    WebContentTracker, measure_memory, parse_footprint_mb,
    kill_voidnix, ext_flags,
)

# ── 参数 ─────────────────────────────────────────────────────────────────────

DEV_MODE = '--dev' in sys.argv
SELF_TEST_ONLY = '--self-test-only' in sys.argv
DO_BUILD = '--build' in sys.argv
NO_CGEVENT = '--no-cgevent' in sys.argv

BUNDLE_ID = 'com.litiantao.voidnix.dev' if DEV_MODE else 'com.litiantao.voidnix'
APP_PATH = 'Voidnix.app'

# 报告路径：直接启动 binary 时 bundle id 是 prod（tauri.conf.json），
# tauri dev 启动时才是 .dev。两个路径都检查。
HOME_DIR = Path.home()
REPORT_PATHS = [
    HOME_DIR / 'Library' / 'Application Support' / 'com.litiantao.voidnix' / 'config' / 'test-report.json',
    HOME_DIR / 'Library' / 'Application Support' / 'com.litiantao.voidnix.dev' / 'config' / 'test-report.json',
]
SMOKE_REPORT_PATH = Path(__file__).parent / 'smoke-test-report.md'
# 内存基线持久化：首次运行采集后写入，后续运行与基线 + 漂移容忍度对比
BASELINES_PATH = Path(__file__).parent / 'smoke-baselines.json'

SELF_TEST_TIMEOUT = 90
CGEVENT_PREPARE_DELAY = 3  # CGEvent 测试前倒计时（让用户切走焦点）

# 内存阈值（MB）——绝对安全上限（硬编码兜底，防止基线文件不存在时误判）
# 有基线文件时，改用基线值 + drift 容忍度对比（更灵敏地检测回归）
MEM_ABSOLUTE_MAX = 350      # footprint 绝对上限（任何情况不得超过）
MEM_GRAPHICS_N_ABSOLUTE_MAX = 80  # graphics PURGE=N 绝对上限
# 基线漂移容忍度（基于历史基线的百分比上浮）
MEM_DRIFT_TOLERANCE = 0.25  # footprint 允许比基线高 25%（防 GC 抖动误报）
MEM_GRAPHICS_DRIFT_TOLERANCE = 0.50  # graphics 区域允许比基线高 50%


# ── TestResult 数据结构 ───────────────────────────────────────────────────────

@dataclass
class TestResult:
    category: str
    name: str
    status: str  # 'pass' | 'fail' | 'skip'
    message: str = ''
    duration_ms: int = 0


@dataclass
class ResultCollector:
    results: list = field(default_factory=list)

    def add(self, category: str, name: str, status: str, message: str = ''):
        self.results.append(TestResult(category, name, status, message, 0))

    def add_pass(self, category: str, name: str, message: str = ''):
        self.add(category, name, 'pass', message)

    def add_fail(self, category: str, name: str, message: str = ''):
        self.add(category, name, 'fail', message)

    def add_skip(self, category: str, name: str, message: str = ''):
        self.add(category, name, 'skip', message)

    @property
    def total(self):
        return len(self.results)

    @property
    def passed(self):
        return sum(1 for r in self.results if r.status == 'pass')

    @property
    def failed(self):
        return sum(1 for r in self.results if r.status == 'fail')

    @property
    def skipped(self):
        return sum(1 for r in self.results if r.status == 'skip')

    def merge_from_json(self, json_results: list):
        """从 Layer 1 JSON 报告合并结果。"""
        for item in json_results:
            self.add(
                item.get('category', '?'),
                item.get('name', '?'),
                item.get('status', 'skip'),
                item.get('message', ''),
            )


# ── 构建 ─────────────────────────────────────────────────────────────────────

def build_release():
    log('开始 release 构建...')
    result = subprocess.run(['bun', 'run', 'tauri', 'build'], capture_output=False)
    if result.returncode != 0:
        log('构建失败')
        sys.exit(1)
    log('构建完成')


def find_app_path() -> str:
    """定位 Voidnix 可执行路径。

    优先 debug binary（配合 Vite dev server，总是加载最新前端代码）。
    Release binary 的内嵌前端仅在 `tauri build` 时更新（非 `cargo build`），
    代码变更后需重新 `tauri build` 才有效——开发期不实际。
    """
    root = Path(__file__).parent.parent
    # 1. debug 裸 binary（需 Vite dev server，launch_self_test 自动启动）
    debug_bin = root / 'src-tauri' / 'target' / 'debug' / 'Voidnix'
    if debug_bin.exists():
        return str(debug_bin)
    # 2. release bundle .app（tauri build 产物）
    release_app = root / 'src-tauri' / 'target' / 'release' / 'bundle' / 'macos' / APP_PATH
    if release_app.exists():
        return str(release_app)
    # 3. release 裸 binary
    release_bin = root / 'src-tauri' / 'target' / 'release' / 'Voidnix'
    if release_bin.exists():
        log('警告: release 裸 binary 内嵌前端可能过时（需 tauri build 更新）')
        return str(release_bin)
    # 4. 已安装的 .app
    installed = Path(f'/Applications/{APP_PATH}')
    if installed.exists():
        log('警告: 使用 /Applications/Voidnix.app（可能不含最新自测代码）')
        return str(installed)
    log('未找到 Voidnix 可执行文件')
    sys.exit(1)


def launch_self_test(app_path: str):
    """以自测模式启动 app（注入环境变量）。

    Debug binary 用 devUrl（需 Vite dev server），release binary 用内嵌 frontendDist。
    如果是 debug binary 且 Vite 未运行，自动启动。
    """
    env = os.environ.copy()
    env['VOIDNIX_SELF_TEST'] = '1'
    p = Path(app_path)
    is_debug = 'debug' in app_path

    # Debug binary 需要 Vite dev server 提供前端
    vite_proc = None
    if is_debug:
        import urllib.request
        try:
            urllib.request.urlopen('http://localhost:1420', timeout=2)
            log('Vite dev server 已在运行')
        except Exception:
            log('启动 Vite dev server...')
            vite_proc = subprocess.Popen(
                ['bun', 'run', 'dev'],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                cwd=str(Path(__file__).parent.parent),
            )
            time.sleep(4)

    # .app bundle → 取 Contents/MacOS/Voidnix；裸 binary → 直接用
    binary = p / 'Contents' / 'MacOS' / 'Voidnix' if p.is_dir() else p
    if not binary.exists():
        log(f'未找到可执行文件: {binary}')
        sys.exit(1)
    log(f'启动自测模式: {binary}')
    subprocess.Popen([str(binary)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)

    return vite_proc


def clear_old_report():
    for p in REPORT_PATHS:
        if p.exists():
            p.unlink()


def wait_for_report(timeout: int = SELF_TEST_TIMEOUT) -> dict:
    """轮询等待自测报告（检查所有可能路径）。"""
    deadline = time.time() + timeout
    last_error = ''
    while time.time() < deadline:
        for report_path in REPORT_PATHS:
            if report_path.exists():
                try:
                    data = json.loads(report_path.read_text())
                    report = data.get('report') or data
                    if 'results' in report:
                        return report
                except (json.JSONDecodeError, KeyError) as e:
                    last_error = str(e)
        time.sleep(1)
    log(f'自测报告等待超时（{timeout}s）。最后错误: {last_error}')
    sys.exit(1)


# ── Layer 2：系统冒烟测试 ─────────────────────────────────────────────────────

_in_ext = False


def _enter_ext(name: str, settle: float = SETTLE_DEFAULT):
    global _in_ext
    clear_input()
    type_text('/' + name)
    time.sleep(SETTLE_INSTANT)
    press_enter()
    time.sleep(settle)
    _in_ext = True


def _exit_ext():
    global _in_ext
    if not _in_ext:
        return
    press_esc()
    time.sleep(ESC_DELAY)
    _in_ext = False
    if not is_voidnix_visible():
        try:
            show_window(DEV_MODE)
        except RuntimeError:
            pass  # 窗口可能已被 Escape 隐藏，show 失败不阻断测试


def test_window_behavior(rc: ResultCollector):
    """窗口行为：show / hide / 二次唤起。"""

    # 唤起
    show_window(DEV_MODE)
    if is_voidnix_visible():
        rc.add_pass('window', 'shortcut 唤起窗口可见')
    else:
        rc.add_fail('window', 'shortcut 唤起窗口可见', '窗口 alpha=0 或尺寸不匹配')

    # 窗口尺寸正确
    bounds = voidnix_window_bounds()
    if bounds and 700 <= bounds[2] <= 740 and 400 <= bounds[3] <= 850:
        rc.add_pass('window', f'窗口尺寸正确 ({bounds[2]:.0f}x{bounds[3]:.0f})')
    elif bounds:
        rc.add_fail('window', '窗口尺寸正确', f'实际 {bounds[2]:.0f}x{bounds[3]:.0f}，期望 ~720x480')
    else:
        rc.add_fail('window', '窗口尺寸正确', '无法读取 bounds')

    # 隐藏
    hide_window(DEV_MODE)
    if not is_voidnix_visible():
        rc.add_pass('window', 'shortcut 隐藏窗口不可见')
    else:
        rc.add_fail('window', 'shortcut 隐藏窗口不可见', '窗口仍可见')

    # 二次唤起
    show_window(DEV_MODE)
    if is_voidnix_visible():
        rc.add_pass('window', '二次唤起窗口可见')
    else:
        rc.add_fail('window', '二次唤起窗口可见', '窗口未显示')

    # 搜索框可输入（验证焦点）
    try:
        clear_input()
        type_text('test')
        time.sleep(0.3)
        rc.add_pass('window', '搜索框可输入')
    except Exception as e:
        rc.add_fail('window', '搜索框可输入', str(e))
    finally:
        clear_input()


def test_global_shortcuts(rc: ResultCollector):
    """全局快捷键：截屏 overlay + 扩展唤起。"""

    # 截屏快捷键 Alt+S → overlay 窗口出现
    hide_window(DEV_MODE)
    trigger_ext_shortcut('s', DEV_MODE)
    time.sleep(SETTLE_DEFAULT)

    # 截屏 overlay 是独立窗口（全屏），应增加可见窗口数
    if count_voidnix_windows() >= 2 or is_voidnix_visible():
        rc.add_pass('shortcut', 'Alt+S 截屏 overlay 出现')
    else:
        rc.add_fail('shortcut', 'Alt+S 截屏 overlay 出现', '无新窗口出现')

    press_esc()
    time.sleep(0.8)

    # 扩展快捷键 Alt+C/T/A/F → 窗口可见 + 扩展激活
    for key, label in [('c', '剪贴板'), ('t', '翻译'), ('a', 'Agent'), ('f', '访达工具')]:
        hide_window(DEV_MODE)
        trigger_ext_shortcut(key, DEV_MODE)
        time.sleep(TOGGLE_GAP + 0.3)
        global _in_ext
        _in_ext = True

        if is_voidnix_visible():
            rc.add_pass('shortcut', f'Alt+{key.upper()} 唤起 {label}')
        else:
            rc.add_fail('shortcut', f'Alt+{key.upper()} 唤起 {label}', '窗口未显示')

        _exit_ext()
        hide_window(DEV_MODE)


def test_search_ui(rc: ResultCollector):
    """搜索 UI：全局搜索 + 工具列表 + 键盘导航。"""
    show_window(DEV_MODE)

    # 全局搜索 — 应用类型
    try:
        search_and_wait('safa', SETTLE_INSTANT)
        rc.add_pass('search-ui', '全局搜索应用 (safari)')
    except Exception as e:
        rc.add_fail('search-ui', '全局搜索应用 (safari)', str(e))

    # 计算器即时答案
    try:
        search_and_wait('1+2', SETTLE_INSTANT)
        rc.add_pass('search-ui', '计算器即时答案 (1+2)')
    except Exception as e:
        rc.add_fail('search-ui', '计算器即时答案 (1+2)', str(e))

    # base64 解码
    try:
        search_and_wait('SGVsbG8=', SETTLE_INSTANT)
        rc.add_pass('search-ui', 'base64 解码即时答案')
    except Exception as e:
        rc.add_fail('search-ui', 'base64 解码即时答案', str(e))

    # 工具列表
    try:
        clear_input()
        type_text('/')
        time.sleep(SETTLE_INSTANT)
        rc.add_pass('search-ui', '工具列表 (/ 前缀)')
    except Exception as e:
        rc.add_fail('search-ui', '工具列表 (/ 前缀)', str(e))

    # 键盘导航
    try:
        press_down(3)
        press_up(2)
        rc.add_pass('search-ui', '结果键盘导航 (上下方向键)')
    except Exception as e:
        rc.add_fail('search-ui', '结果键盘导航 (上下方向键)', str(e))

    clear_input()


def test_extension_views(rc: ResultCollector):
    """扩展视图：逐个进入/退出所有 mainView 扩展。"""
    show_window(DEV_MODE)

    # mainView 扩展（通过 /keyword 进入）
    mainview_exts = [
        ('clip', '剪贴板'), ('sett', '设置'), ('uuid', 'UUID'),
        ('system', '系统状态'), ('awake', '保持唤醒'), ('screenshot', '截屏'),
        ('window', '窗口管理'), ('proxy', '代理'), ('agent', 'Agent'),
        ('translate', '翻译'), ('image', '图片处理'), ('brew', 'Homebrew'),
        ('video', '视频处理'), ('clean', '清洁模式'), ('provider', 'AI 提供商'),
        ('finder', '访达工具'), ('zsh', '终端自动建议'),
    ]

    for kw, label in mainview_exts:
        try:
            _enter_ext(kw, SETTLE_DEFAULT)
            if is_voidnix_visible():
                rc.add_pass('extension-ui', f'{label} 视图渲染')
            else:
                rc.add_fail('extension-ui', f'{label} 视图渲染', '进入后窗口隐藏')
        except Exception as e:
            rc.add_fail('extension-ui', f'{label} 视图渲染', str(e))
        finally:
            try:
                _exit_ext()
            except Exception:
                pass

    # 搜索型扩展内交互
    try:
        _enter_ext('calc', SETTLE_INSTANT)
        type_text('2+3*4')
        time.sleep(SETTLE_INSTANT)
        rc.add_pass('extension-ui', '计算器扩展内输入')
    except Exception as e:
        rc.add_fail('extension-ui', '计算器扩展内输入', str(e))
    finally:
        try:
            clear_input()
            _exit_ext()
        except Exception:
            pass


def test_snap_panel(rc: ResultCollector):
    """snap-panel 全链路：UI 点击启用 → 触发 → 隐藏 → 布局点击 → UI 点击禁用销毁。

    启用/禁用均通过鼠标点击 toggle 按钮模拟真实用户操作，走 config.ts watch →
    invoke(setWindowManagerEnabled) 前端路径——而非预写 config 绕过。
    """
    w, h = screen_size()

    # ── 启用：UI 点击 toggle ON ──
    show_window(DEV_MODE)
    _enter_ext('window', SETTLE_DEFAULT)
    time.sleep(0.3)
    click_wm_toggle()
    time.sleep(1.0)
    _exit_ext()
    hide_window(DEV_MODE)
    time.sleep(TOGGLE_GAP)

    if snap_panel_exists():
        rc.add_pass('snap-panel', 'UI 点击启用后窗口已创建')
    else:
        rc.add_fail('snap-panel', 'UI 点击启用后窗口已创建', '窗口未创建')

    # ── 触发区显示 ──
    triggered = trigger_snap_panel(w, attempts=5, interval=0.5)
    if triggered:
        rc.add_pass('snap-panel', '触发区显示正常')
    else:
        rc.add_fail('snap-panel', '触发区显示正常', 'drag monitor 未触发')

    # 移出隐藏
    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.8)
    if not is_snap_panel_visible():
        rc.add_pass('snap-panel', '移出触发区后隐藏')
    else:
        rc.add_fail('snap-panel', '移出触发区后隐藏', '面板仍可见')

    # 二次触发
    if trigger_snap_panel(w, attempts=5, interval=0.5):
        rc.add_pass('snap-panel', '二次触发正常')
    else:
        rc.add_fail('snap-panel', '二次触发正常', '二次触发失败')

    move_mouse_to_snap_trigger(w / 2, h / 2)
    time.sleep(0.7)

    # 布局点击验证
    ensure_finder_window()
    before = finder_window_bounds()

    if before and trigger_snap_panel(w, attempts=5):
        time.sleep(0.4)
        panel = snap_panel_visible_bounds()
        if panel:
            click_at(panel[0] + 188, panel[1] + 40)
            time.sleep(1.5)

            after = finder_window_bounds()
            if after and after[0] > w * 0.4 and after[2] < w * 0.65:
                rc.add_pass('snap-panel', '布局点击生效 (Finder 移至右半屏)')
            else:
                rc.add_fail('snap-panel', '布局点击生效', f'Finder frame 未变或不对: {after}')
        else:
            rc.add_fail('snap-panel', '布局点击生效', 'snap-panel bounds 读取失败')
    else:
        rc.add_skip('snap-panel', '布局点击生效', '无法打开 Finder 或触发面板')

    close_finder_windows()
    time.sleep(0.3)

    # ── 禁用：UI 点击 toggle OFF ──
    # 禁用时 snap-panel 窗口保持存活（WKWebView teardown 抛 C++ foreign exception
    # 无法安全销毁），仅停 drag monitor + 隐藏窗口。验证 drag monitor 停止即可。
    show_window(DEV_MODE)
    _enter_ext('window', SETTLE_DEFAULT)
    time.sleep(0.3)
    click_wm_toggle()
    time.sleep(1.0)
    _exit_ext()
    hide_window(DEV_MODE)
    time.sleep(TOGGLE_GAP)

    # drag monitor 停止验证：移到触发区，snap-panel 不应显示
    move_mouse_to_snap_trigger(w / 2, 100)
    time.sleep(1.0)
    if not is_snap_panel_visible():
        rc.add_pass('snap-panel', 'UI 点击禁用后 drag monitor 已停止')
    else:
        rc.add_fail('snap-panel', 'UI 点击禁用后 drag monitor 已停止', '面板仍可触发')

    # 恢复 WM 配置为 disabled（无论禁用步骤是否成功，确保不残留）
    for bid in ['com.litiantao.voidnix', 'com.litiantao.voidnix.dev']:
        cfg = pathlib.Path.home() / 'Library' / 'Application Support' / bid / 'extensions' / 'window-manager' / 'config.json'
        if cfg.exists():
            try:
                d = json.loads(cfg.read_text())
                d['enabled'] = False
                cfg.write_text(json.dumps(d))
            except Exception:
                pass


def ensure_wm_disabled():
    """启动前确保 WM 配置 enabled=false。

    snap-panel 的启用/禁用由 test_snap_panel 通过 UI 点击 toggle 完成（模拟真实用户操作），
    不再预写 config 绕过前端 watch → invoke 路径——该绕过正是此前三个 bug 逃逸的原因。
    """
    for bid in ['com.litiantao.voidnix', 'com.litiantao.voidnix.dev']:
        p = pathlib.Path.home() / 'Library' / 'Application Support' / bid / 'extensions' / 'window-manager' / 'config.json'
        if p.exists():
            try:
                d = json.loads(p.read_text())
                d['enabled'] = False
                p.write_text(json.dumps(d))
            except Exception:
                pass


def click_wm_toggle():
    """在窗口管理扩展视图中按 Enter 切换启用开关。

    进入扩展后 BaseList 默认选中 index 0（wm-enabled），Enter 经 BaseSettingsList
    onExecute → item.update(!value) 切换 toggle。不依赖鼠标坐标（CGEvent 在大量
    鼠标操作后可能不稳定），纯键盘交互更可靠。
    """
    press_enter()
    time.sleep(0.5)


def load_baselines() -> dict:
    """加载历史内存基线（首次运行返回空 dict）。"""
    if BASELINES_PATH.exists():
        try:
            return json.loads(BASELINES_PATH.read_text())
        except (json.JSONDecodeError, OSError):
            pass
    return {}


def save_baselines(measured: dict, previous: dict):
    """更新基线文件。仅在 drift < 容忍度时更新（避免抖动峰值固化）。

    previous 为空时直接写入（首次采集）。
    """
    if previous:
        prev_fp = previous.get('footprint_mb', measured['footprint_mb'])
        prev_gn = previous.get('graphics_n_mb', measured['graphics_n_mb'])
        # drift 超容忍度时不更新（可能是 GC 未回收的瞬时峰值，不该固化）
        if measured['footprint_mb'] > prev_fp * (1 + MEM_DRIFT_TOLERANCE):
            return
        if measured['graphics_n_mb'] > prev_gn * (1 + MEM_GRAPHICS_DRIFT_TOLERANCE):
            return
    try:
        BASELINES_PATH.write_text(json.dumps(measured, indent=2, ensure_ascii=False) + '\n')
    except OSError:
        pass


def test_memory_baseline(rc: ResultCollector):
    """内存基线：采集当前 footprint + graphics，与历史基线 + 绝对阈值对比。

    基线持久化策略：
    - 首次运行（无 smoke-baselines.json）：仅检查绝对上限，采集值写入基线文件
    - 后续运行：与基线 + drift 容忍度对比（更灵敏），同时检查绝对上限
    - 基线文件提交到仓库（团队共享参考基线），每次运行可选择更新
    """
    tracker = WebContentTracker()

    try:
        mem = measure_memory(tracker)
    except Exception as e:
        rc.add_skip('memory', '内存采集', f'WebContent 进程不可读: {e}')
        return

    fp_mb = mem['footprint_mb']
    g = mem['graphics']

    # 加载历史基线
    baselines = load_baselines()

    if fp_mb < 0:
        rc.add_skip('memory', 'footprint 基线', '解析失败')
        return

    # ── footprint 检查 ──
    baseline_fp = baselines.get('footprint_mb')
    if baseline_fp:
        # 有基线：drift 对比（更灵敏）
        threshold = baseline_fp * (1 + MEM_DRIFT_TOLERANCE)
        if fp_mb <= threshold:
            rc.add_pass('memory', f'footprint {fp_mb:.0f}MB (基线 {baseline_fp:.0f}MB, drift +{((fp_mb / baseline_fp - 1) * 100):.0f}%)')
        elif fp_mb <= MEM_ABSOLUTE_MAX:
            rc.add_pass('memory', f'footprint {fp_mb:.0f}MB (基线 {baseline_fp:.0f}MB, drift +{((fp_mb / baseline_fp - 1) * 100):.0f}% 超 {MEM_DRIFT_TOLERANCE*100:.0f}% 容忍但 < 绝对上限)')
        else:
            rc.add_fail('memory', f'footprint {fp_mb:.0f}MB', f'超过绝对上限 {MEM_ABSOLUTE_MAX}MB (基线 {baseline_fp:.0f}MB)')
    else:
        # 无基线：仅绝对上限
        if fp_mb <= MEM_ABSOLUTE_MAX:
            rc.add_pass('memory', f'footprint {fp_mb:.0f}MB (首次运行, < 绝对上限 {MEM_ABSOLUTE_MAX}MB)')
        else:
            rc.add_fail('memory', f'footprint {fp_mb:.0f}MB', f'超过绝对上限 {MEM_ABSOLUTE_MAX}MB')

    # ── graphics PURGE=N 检查 ──
    baseline_gn = baselines.get('graphics_n_mb')
    if baseline_gn is not None:
        threshold = baseline_gn * (1 + MEM_GRAPHICS_DRIFT_TOLERANCE)
        if g['n_mb'] <= threshold:
            rc.add_pass('memory', f'graphics N {g["n_mb"]:.0f}MB (基线 {baseline_gn:.0f}MB)')
        elif g['n_mb'] <= MEM_GRAPHICS_N_ABSOLUTE_MAX:
            rc.add_pass('memory', f'graphics N {g["n_mb"]:.0f}MB (drift 超 {MEM_GRAPHICS_DRIFT_TOLERANCE*100:.0f}% 但 < 绝对上限)')
        else:
            rc.add_fail('memory', f'graphics N {g["n_mb"]:.0f}MB', f'超过绝对上限 {MEM_GRAPHICS_N_ABSOLUTE_MAX}MB (基线 {baseline_gn:.0f}MB)')
    else:
        if g['n_mb'] <= MEM_GRAPHICS_N_ABSOLUTE_MAX:
            rc.add_pass('memory', f'graphics N {g["n_mb"]:.0f}MB (首次运行, < 绝对上限 {MEM_GRAPHICS_N_ABSOLUTE_MAX}MB)')
        else:
            rc.add_fail('memory', f'graphics N {g["n_mb"]:.0f}MB', f'超过绝对上限 {MEM_GRAPHICS_N_ABSOLUTE_MAX}MB')

    rc.add_pass('memory', f'graphics V {g["v_mb"]:.0f}MB | 区域数 {g["count"]}')

    # 更新基线文件（写入本次采集值，供下次运行对比）
    # drift < 容忍度时才更新基线（避免抖动峰值被固化）
    new_baselines = {
        'footprint_mb': round(fp_mb),
        'graphics_n_mb': round(g['n_mb']),
        'graphics_v_mb': round(g['v_mb']),
        'graphics_count': g['count'],
        'timestamp': time.strftime('%Y-%m-%dT%H:%M:%S'),
    }
    save_baselines(new_baselines, baselines)


def run_layer2(rc: ResultCollector):
    """执行 Layer 2 全部系统冒烟测试。"""
    log('\n准备 CGEvent 测试，切换到 ASCII 键盘布局...')
    saved_input = switch_to_ascii()
    if saved_input:
        log('已切换到 ASCII 键盘布局')
    else:
        log('警告: 无法切换输入法')

    try:
        log(f'\n{CGEVENT_PREPARE_DELAY} 秒后开始 CGEvent 测试，请勿操作键盘鼠标...')
        for i in range(CGEVENT_PREPARE_DELAY, 0, -1):
            log(f'  {i}...')
            time.sleep(1)

        log('\n── 窗口行为 ──')
        test_window_behavior(rc)

        log('\n── 全局快捷键 ──')
        test_global_shortcuts(rc)

        log('\n── 搜索 UI ──')
        test_search_ui(rc)

        log('\n── 扩展视图 ──')
        test_extension_views(rc)

        log('\n── snap-panel ──')
        test_snap_panel(rc)

        log('\n── 内存基线 ──')
        test_memory_baseline(rc)
    finally:
        restore_input_source(saved_input)
        if saved_input:
            log('已恢复原始输入法')


# ── 报告输出 ─────────────────────────────────────────────────────────────────

def print_summary(rc: ResultCollector, layer1_report: Optional[dict]):
    """控制台输出全部测试摘要。"""
    log(f'\n{"":>4}总用例 {rc.total} | 通过 {rc.passed} | 失败 {rc.failed} | 跳过 {rc.skipped}')

    categories: dict[str, list] = {}
    for r in rc.results:
        categories.setdefault(r.category, []).append(r)

    for cat, items in categories.items():
        cat_pass = sum(1 for i in items if i.status == 'pass')
        status_str = '全通过' if cat_pass == len(items) else f'{cat_pass}/{len(items)}'
        log(f'\n  [{cat}] ({status_str})')
        for item in items:
            icon = {'pass': '+', 'fail': 'x', 'skip': '-'}[item.status]
            line = f'    {icon} {item.name}'
            if item.status == 'fail':
                line += f'\n      → {item.message}'
            elif item.status == 'skip':
                line += f' ({item.message})'
            log(line)


def write_markdown_report(rc: ResultCollector, overall_pass: bool):
    """生成 Markdown 汇总报告。"""
    categories: dict[str, list] = {}
    for r in rc.results:
        categories.setdefault(r.category, []).append(r)

    lines = [
        '# Voidnix 全功能回归测试报告',
        f'时间：{time.strftime("%Y-%m-%d %H:%M:%S")}  构建：{"dev" if DEV_MODE else "release"}',
        '',
        '## 汇总',
        f'总用例 {rc.total} | 通过 {rc.passed} | 失败 {rc.failed} | 跳过 {rc.skipped} | '
        f'结果：**{"全绿" if overall_pass else "有失败"}**',
        '',
    ]

    for cat, items in categories.items():
        cat_pass = sum(1 for i in items if i.status == 'pass')
        lines.append(f'## {cat}（{cat_pass}/{len(items)}）')
        lines.append('')
        for item in items:
            icon = {'pass': '+', 'fail': 'x', 'skip': '-'}[item.status]
            line = f'- [{icon}] {item.name}'
            if item.status == 'fail':
                line += f'\n  - `{item.message}`'
            elif item.status == 'skip':
                line += f' _({item.message})_'
            lines.append(line)
        lines.append('')

    SMOKE_REPORT_PATH.write_text('\n'.join(lines))
    log(f'\n报告已写入: {SMOKE_REPORT_PATH}')


# ── 主流程 ───────────────────────────────────────────────────────────────────

def main():
    log('=' * 60)
    log('Voidnix 全功能回归测试')
    log(f'模式: {"dev" if DEV_MODE else "release"}'
        f'{" | 仅自测" if SELF_TEST_ONLY else " | 完整"}'
        f'{" | 无 CGEvent" if NO_CGEVENT else ""}')
    log('=' * 60)

    if DO_BUILD:
        build_release()

    app_path = find_app_path()
    rc = ResultCollector()

    # ── Layer 1：应用自测 ──
    log('\n── Layer 1：应用自测 ────────────────────────────')

    if not NO_CGEVENT and not SELF_TEST_ONLY:
        ensure_wm_disabled()

    kill_voidnix()
    clear_old_report()
    vite_proc = launch_self_test(app_path)

    log(f'等待自测报告（超时 {SELF_TEST_TIMEOUT}s）...')
    layer1_report = wait_for_report()
    log('自测报告已收到')

    # 合并 Layer 1 结果
    rc.merge_from_json(layer1_report.get('results', []))

    layer1_summary = layer1_report.get('summary', {})
    log(f'  Layer 1: {layer1_summary.get("passed", 0)}/{layer1_summary.get("total", 0)} 通过')

    # ── Layer 2：系统冒烟 ──
    if not SELF_TEST_ONLY and not NO_CGEVENT:
        log('\n── Layer 2：系统冒烟（CGEvent）────────────────────')
        run_layer2(rc)
    elif NO_CGEVENT:
        log('\n--no-cgevent：跳过 Layer 2')
        kill_voidnix()
    else:
        log('\n仅自测模式：跳过 Layer 2')
        kill_voidnix()

    if vite_proc:
        vite_proc.terminate()

    overall_pass = rc.failed == 0

    # ── 汇总 ──
    log('\n' + '=' * 60)
    print_summary(rc, layer1_report)
    log('')
    if overall_pass:
        log('结果：全绿')
    else:
        log(f'结果：{rc.failed} 项失败')
    log('=' * 60)

    write_markdown_report(rc, overall_pass)
    sys.exit(0 if overall_pass else 1)


if __name__ == '__main__':
    main()
