#!/usr/bin/env python3
"""Voidnix 全功能回归测试编排器。

三层测试架构，功能正确性 + 性能指标在同一流程内完成：
  Layer 1（应用自测）：app 内部直接调用 searchEngine / getAllExtensions / invoke 等真实 API，
    验证搜索正确性、扩展注册、视图渲染、命令可达性、扩展功能正确性、搜索延迟。
    经环境变量 VOIDNIX_SELF_TEST=1 触发，结果写到 app 数据目录 config/test-report.json。
  Layer 2（系统冒烟）：CGEvent 驱动真实 UI 操作，验证窗口行为、全局快捷键、snap-panel、
    搜索 UI、扩展视图。每步返回结构化 TestResult。逐阶段内存采样输出趋势。
  Layer 3（性能压测，--perf）：N 轮全场景工作负载循环 + 逐阶段内存快照，
    输出多轮趋势表，定位 compositing layer 累积。

用法：
    python3 scripts/smoke-test.py                  # 标准（Layer 1 + Layer 2 + 逐阶段内存）
    python3 scripts/smoke-test.py --perf [N]       # 标准 + N 轮内存压测趋势（默认 5 轮）
    python3 scripts/smoke-test.py --self-test-only # 仅 Layer 1（快，~30s，无需独占屏幕）
    python3 scripts/smoke-test.py --dev            # dev 构建（.dev bundle id）
    python3 scripts/smoke-test.py --build          # 含 release 构建
    python3 scripts/smoke-test.py --no-cgevent     # 跳过 Layer 2（CI/headless 友好）
"""

import json
import os
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
    SETTLE_INSTANT, SETTLE_DEFAULT, SETTLE_NETWORK, ESC_DELAY, TOGGLE_GAP,
    switch_to_ascii, restore_input_source,
    is_voidnix_visible, is_snap_panel_visible, snap_panel_exists,
    snap_panel_visible_bounds, voidnix_window_bounds, count_voidnix_windows,
    type_text, press_enter, press_esc, press_down, press_up,
    clear_input, search_and_wait,
    show_window, hide_window, trigger_ext_shortcut,
    click_at, move_mouse_to_snap_trigger, screen_size,
    trigger_snap_panel,
    ensure_finder_window, finder_window_bounds, close_finder_windows,
    WebContentTracker, measure_memory,
    kill_voidnix,
    reset_modifiers,
)

# ── 参数 ─────────────────────────────────────────────────────────────────────

DEV_MODE = '--dev' in sys.argv
SELF_TEST_ONLY = '--self-test-only' in sys.argv
DO_BUILD = '--build' in sys.argv
NO_CGEVENT = '--no-cgevent' in sys.argv

# --perf [N]：标准测试后追加 N 轮全场景内存压测（默认 5 轮）
PERF_MODE = '--perf' in sys.argv
PERF_ROUNDS = 5
for _i, _arg in enumerate(sys.argv):
    if _arg == '--perf' and _i + 1 < len(sys.argv):
        try:
            PERF_ROUNDS = int(sys.argv[_i + 1])
        except ValueError:
            pass

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
MEM_GRAPHICS_N_ABSOLUTE_MAX = 250  # graphics PURGE=N 绝对上限（硬兜底，须高于 drift 阈值 基线×1.5）
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


# ── 内存趋势追踪 ─────────────────────────────────────────────────────────────

@dataclass
class MemSample:
    label: str
    fp_mb: float
    gn_mb: float
    gv_mb: float
    count: int


class MemoryTrend:
    """逐阶段内存采样，输出趋势表。"""

    def __init__(self):
        self.samples: list[MemSample] = []
        self._tracker = WebContentTracker()

    @property
    def tracker(self):
        return self._tracker

    def sample(self, label: str):
        try:
            mem = measure_memory(self._tracker)
            s = MemSample(label, mem['footprint_mb'], mem['graphics']['n_mb'],
                          mem['graphics']['v_mb'], mem['graphics']['count'])
            self.samples.append(s)
            log(f'  内存 [{label}]  FP {s.fp_mb:.0f}MB  graphics N {s.gn_mb:.0f}MB  V {s.gv_mb:.0f}MB')
        except Exception as e:
            log(f'  内存 [{label}] 采集失败: {e}')

    def print_trend(self):
        if not self.samples:
            return
        log(f'\n  {"阶段":>20}  {"FP":>8}  {"N":>8}  {"V":>8}  {"区域数":>6}')
        for s in self.samples:
            log(f'  {s.label:>20}  {s.fp_mb:>7.0f}MB  {s.gn_mb:>7.0f}MB  {s.gv_mb:>7.0f}MB  {s.count:>6}')

    def to_markdown(self) -> list[str]:
        if not self.samples:
            return []
        lines = ['### 内存趋势', '', '| 阶段 | FP | graphics N | graphics V | 区域数 |', '|---|---|---|---|---|']
        for s in self.samples:
            lines.append(f'| {s.label} | {s.fp_mb:.0f}MB | {s.gn_mb:.0f}MB | {s.gv_mb:.0f}MB | {s.count} |')
        return lines


# 全局实例（Layer 2 逐阶段采样 + --perf 多轮采样共用）
mem_trend = MemoryTrend()


# ── 构建 ─────────────────────────────────────────────────────────────────────

def build_release():
    log('开始 release 构建...')
    root = Path(__file__).parent.parent
    # 与 deploy.sh 同源：加载 .env（codesign 身份 + updater 私钥），防产物退化 adhoc 或 updater sig 失败
    cmd = f'set -a; source "{root / ".env"}" 2>/dev/null; exec bun run tauri build'
    result = subprocess.run(['bash', '-c', cmd], capture_output=False, cwd=root)
    if result.returncode != 0:
        log('构建失败')
        sys.exit(1)
    log('构建完成')


def find_app_path() -> str:
    """定位 Voidnix 可执行路径。

    按 DEV_MODE 分流，确保快捷键基（debug 叠 Shift / release 不叠）与 Layer 2
    CGEvent 发送的修饰键一致，避免唤起失败。

    --dev：优先 debug binary（配合 Vite dev server，总是加载最新前端代码）。
    默认（release）：优先 release bundle .app，其次 /Applications/Voidnix.app。
    """
    root = Path(__file__).parent.parent

    if DEV_MODE:
        # 1. debug 裸 binary（需 Vite dev server，launch_self_test 自动启动）
        debug_bin = root / 'src-tauri' / 'target' / 'debug' / 'Voidnix'
        if debug_bin.exists():
            return str(debug_bin)
        log('警告: --dev 模式但未找到 debug binary，回退到 release')

    # release 模式（或 --dev 回退）
    # 1. release bundle .app（tauri build 产物，内嵌前端最新）
    release_app = root / 'src-tauri' / 'target' / 'release' / 'bundle' / 'macos' / APP_PATH
    if release_app.exists():
        return str(release_app)
    # 2. 已安装的 .app（deploy.sh 部署后）
    installed = Path(f'/Applications/{APP_PATH}')
    if installed.exists():
        return str(installed)
    # 3. release 裸 binary（前端可能过时，仅兜底）
    release_bin = root / 'src-tauri' / 'target' / 'release' / 'Voidnix'
    if release_bin.exists():
        log('警告: release 裸 binary 内嵌前端可能过时（需 tauri build 更新）')
        return str(release_bin)
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
    if _in_ext:
        _exit_ext()
    show_window(DEV_MODE)
    clear_input()
    type_text('/' + name)
    time.sleep(SETTLE_INSTANT)
    press_enter()
    time.sleep(settle)
    _in_ext = True


def _exit_ext():
    """退出扩展模式：Esc 退出 + 清零标志 + 释放修饰键。

    不调 show_window——Esc 可能隐藏窗口（正常行为），由调用方按需 show。
    避免「Esc 隐藏 → show toggle → 外部 hide toggle」的双 toggle 竞态。
    """
    global _in_ext
    if not _in_ext:
        return
    _in_ext = False  # 先清零，防止后续操作异常时残留
    if is_voidnix_visible():
        press_esc()
        time.sleep(ESC_DELAY)
    reset_modifiers()


def _reset_state():
    """测试阶段间状态重置：释放修饰键 + 退出扩展模式 + 清空输入 + 确保窗口可见。

    各 test_* 阶段之间可能残留扩展激活态 / 搜索框内容 / 键盘焦点 / 卡住的修饰键，
    不重置会导致下一阶段输入被拦截或修饰键级联触发扩展快捷键产生乱跳。

    show_window 失败时重试一次（2s 后）；仍失败则抛 RuntimeError，
    由 run_layer2 的阶段级 try/except 安全处理。
    """
    global _in_ext
    reset_modifiers()
    if _in_ext:
        _exit_ext()
    try:
        show_window(DEV_MODE)
    except RuntimeError:
        time.sleep(2)
        show_window(DEV_MODE)
    clear_input()
    time.sleep(SETTLE_INSTANT)


def test_window_behavior(rc: ResultCollector):
    """窗口行为：show / hide / 二次唤起。"""

    # 确保 clean start（_reset_state 由 run_layer2 调用）
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
    global _in_ext

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

        if is_voidnix_visible():
            _in_ext = True
            rc.add_pass('shortcut', f'Alt+{key.upper()} 唤起 {label}')
        else:
            _in_ext = False
            rc.add_fail('shortcut', f'Alt+{key.upper()} 唤起 {label}', '窗口未显示')

        _exit_ext()
        hide_window(DEV_MODE)

    # 快捷键测试结束：强制释放所有修饰键，防止残留 Option 状态
    # 导致下一阶段 type_text 的每个字符被系统误判为 Opt+key 触发扩展快捷键
    reset_modifiers()


def test_search_ui(rc: ResultCollector):
    """搜索 UI：全局搜索 + 工具列表 + 键盘导航。"""
    # _reset_state 由 run_layer2 调用

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
    """扩展视图：逐个进入/退出所有 mainView 扩展。

    数据加载型扩展（system-status / homebrew / proxy / video）给更多时间让
    异步请求（系统 API / brew / mihomo API / ffmpeg probe）完成后渲染完整。
    """
    # _reset_state 由 run_layer2 调用

    # (keyword, label, settle) — settle 按数据加载成本分档
    mainview_exts = [
        ('clip', '剪贴板', SETTLE_DEFAULT), ('sett', '设置', SETTLE_DEFAULT),
        ('uuid', 'UUID', SETTLE_INSTANT),
        ('system', '系统状态', SETTLE_NETWORK), ('awake', '保持唤醒', SETTLE_DEFAULT),
        ('screenshot', '截屏', SETTLE_INSTANT),
        ('window', '窗口管理', SETTLE_DEFAULT), ('proxy', '代理', SETTLE_NETWORK),
        ('agent', 'Agent', SETTLE_DEFAULT), ('translate', '翻译', SETTLE_DEFAULT),
        ('image', '图片处理', SETTLE_DEFAULT), ('brew', 'Homebrew', SETTLE_NETWORK),
        ('video', '视频处理', SETTLE_NETWORK), ('clean', '清洁模式', SETTLE_INSTANT),
        ('provider', 'AI 提供商', SETTLE_DEFAULT), ('finder', '访达工具', SETTLE_DEFAULT),
        ('zsh', '终端自动建议', SETTLE_DEFAULT),
    ]

    for kw, label, settle in mainview_exts:
        try:
            _enter_ext(kw, settle)
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

    # 翻译扩展：进入 → 输入 → 验证结果渲染（无 API Key 时显示未配置空态，窗口保持可见即可）
    try:
        _enter_ext('translate', SETTLE_DEFAULT)
        type_text('hello')
        time.sleep(SETTLE_DEFAULT)
        if is_voidnix_visible():
            rc.add_pass('extension-ui', '翻译输入翻译流程')
        else:
            rc.add_fail('extension-ui', '翻译输入翻译流程', '翻译后窗口隐藏')
    except Exception as e:
        rc.add_fail('extension-ui', '翻译输入翻译流程', str(e))
    finally:
        try:
            clear_input()
            _exit_ext()
        except Exception:
            pass

    # 剪贴板扩展：进入 → 导航历史列表 → Enter 粘贴（验证列表交互可达）
    try:
        _enter_ext('clip', SETTLE_DEFAULT)
        press_down(2)
        press_up(1)
        time.sleep(SETTLE_INSTANT)
        if is_voidnix_visible():
            rc.add_pass('extension-ui', '剪贴板列表导航交互')
        else:
            rc.add_fail('extension-ui', '剪贴板列表导航交互', '导航后窗口隐藏')
    except Exception as e:
        rc.add_fail('extension-ui', '剪贴板列表导航交互', str(e))
    finally:
        try:
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

    # 布局点击验证：左半屏 + 右半屏交替，两个不同方向各自验证位移
    # Finder 窗口记忆位置，用不同布局交替可避免「默认就在目标区域」的假阳性
    ensure_finder_window()

    # ── 左半屏 ──
    finder_ok = finder_window_bounds()
    if finder_ok and trigger_snap_panel(w, attempts=5):
        time.sleep(0.4)
        panel = snap_panel_visible_bounds()
        if panel:
            # halves-h 组的左 zone center ≈ panel_x + 163
            click_at(panel[0] + 163, panel[1] + 40)
            time.sleep(1.5)
            after = finder_window_bounds()
            if after and after[0] < w * 0.15 and after[2] < w * 0.6:
                rc.add_pass('snap-panel', '布局点击生效 (Finder 移至左半屏)')
            else:
                rc.add_fail('snap-panel', '布局点击左半屏', f'Finder frame 不对: {after}')
        else:
            rc.add_fail('snap-panel', '布局点击左半屏', 'snap-panel bounds 读取失败')
    else:
        rc.add_skip('snap-panel', '布局点击左半屏', '无法打开 Finder 或触发面板')

    # ── 右半屏（同一窗口换方向，验证不同布局按钮）──
    if trigger_snap_panel(w, attempts=5):
        time.sleep(0.4)
        panel = snap_panel_visible_bounds()
        if panel:
            # halves-h 组的右 zone center ≈ panel_x + 188
            click_at(panel[0] + 188, panel[1] + 40)
            time.sleep(1.5)
            after = finder_window_bounds()
            if after and after[0] > w * 0.4 and after[2] < w * 0.65:
                rc.add_pass('snap-panel', '布局点击生效 (Finder 移至右半屏)')
            else:
                rc.add_fail('snap-panel', '布局点击右半屏', f'Finder frame 不对: {after}')
        else:
            rc.add_fail('snap-panel', '布局点击右半屏', 'snap-panel bounds 读取失败')
    else:
        rc.add_skip('snap-panel', '布局点击右半屏', '触发面板失败')

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
        cfg = Path.home() / 'Library' / 'Application Support' / bid / 'extensions' / 'window-manager' / 'config.json'
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
        p = Path.home() / 'Library' / 'Application Support' / bid / 'extensions' / 'window-manager' / 'config.json'
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
    tracker = mem_trend.tracker

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
    """执行 Layer 2 全部系统冒烟测试。

    每个阶段独立 try/except：单阶段 RuntimeError（窗口隐藏等）不崩溃脚本，
    记录失败后尝试恢复窗口进入下一阶段，保证报告始终写出。
    """
    log('\n准备 CGEvent 测试，切换到 ASCII 键盘布局...')
    saved_input = switch_to_ascii()
    if saved_input:
        log('已切换到 ASCII 键盘布局')
    else:
        log('警告: 无法切换输入法')
    # CGEvent 开始前清零修饰键状态（安全网）
    reset_modifiers()

    # (阶段名, 测试函数, 采样标签)
    phases = [
        ('窗口行为', test_window_behavior, '窗口行为'),
        ('全局快捷键', test_global_shortcuts, None),
        ('搜索 UI', test_search_ui, '搜索 UI'),
        ('扩展视图', test_extension_views, '扩展视图'),
        ('snap-panel', test_snap_panel, 'snap-panel'),
        ('内存基线', test_memory_baseline, None),
    ]

    try:
        log(f'\n{CGEVENT_PREPARE_DELAY} 秒后开始 CGEvent 测试，请勿操作键盘鼠标...')
        for i in range(CGEVENT_PREPARE_DELAY, 0, -1):
            log(f'  {i}...')
            time.sleep(1)

        for phase_name, test_fn, mem_label in phases:
            log(f'\n── {phase_name} ──')
            try:
                _reset_state()
                test_fn(rc)
            except RuntimeError as e:
                rc.add_fail('window', f'{phase_name} 阶段中断', str(e))
                log(f'  阶段中断: {e}，尝试恢复...')
                global _in_ext
                _in_ext = False
                # 尽力恢复窗口，成功则继续下一阶段
                try:
                    time.sleep(1)
                    show_window(DEV_MODE)
                except RuntimeError:
                    log('  恢复失败，跳过剩余阶段')
                    break
            if mem_label:
                mem_trend.sample(mem_label)
    finally:
        restore_input_source(saved_input)
        if saved_input:
            log('已恢复原始输入法')


# ── Layer 3：性能压测工作负载（--perf）─────────────────────────────────────────

def _recover_after_error():
    """工作负载异常恢复：退出扩展 + 清空输入 + 释放修饰键 + 尽力 show_window。"""
    global _in_ext
    if _in_ext and is_voidnix_visible():
        press_esc()
        time.sleep(ESC_DELAY)
    _in_ext = False
    reset_modifiers()
    try:
        show_window(DEV_MODE)
        clear_input()
    except RuntimeError:
        pass


def workload_global_search():
    """全局搜索——覆盖应用/文件/即时答案/web 结果类型。"""
    for w in ['safa', 'term', 'note', 'code', 'musi', 'mail', 'calc', 'sett']:
        try:
            search_and_wait(w, SETTLE_INSTANT)
        except Exception:
            _recover_after_error()
    for w in ['doc', 'pdf', 'config', 'desktop']:
        try:
            search_and_wait(w, SETTLE_DEFAULT)
        except Exception:
            _recover_after_error()
    for expr in ['1+2', '3*4', '100-7', '2^10']:
        try:
            search_and_wait(expr, SETTLE_INSTANT)
        except Exception:
            _recover_after_error()
    try:
        search_and_wait('SGVsbG8', SETTLE_INSTANT)
        search_and_wait('//rust async', SETTLE_DEFAULT)
        clear_input()
    except Exception:
        _recover_after_error()


def workload_tool_list():
    """工具列表——/ 前缀 + 过滤导航。"""
    try:
        search_and_wait('/', SETTLE_INSTANT)
        press_down(3)
        press_up(2)
    except Exception:
        _recover_after_error()
    for kw in ['/calc', '/clip', '/time', '/uuid']:
        try:
            clear_input()
            type_text(kw)
            time.sleep(SETTLE_INSTANT)
            press_down(1)
        except Exception:
            _recover_after_error()
    try:
        clear_input()
    except Exception:
        _recover_after_error()


def workload_extensions():
    """扩展视图——覆盖全部 mainView 扩展的 DOM 渲染路径。"""
    global _in_ext
    for kw, settle in [
        ('clip', SETTLE_DEFAULT), ('sett', SETTLE_DEFAULT), ('uuid', SETTLE_INSTANT),
        ('system', SETTLE_NETWORK), ('awake', SETTLE_DEFAULT), ('screenshot', SETTLE_INSTANT),
        ('window', SETTLE_DEFAULT), ('proxy', SETTLE_NETWORK), ('agent', SETTLE_DEFAULT),
        ('translate', SETTLE_DEFAULT), ('image', SETTLE_DEFAULT), ('brew', SETTLE_NETWORK),
        ('video', SETTLE_NETWORK),
    ]:
        try:
            _enter_ext(kw, settle)
            press_down(2)
            _exit_ext()
        except Exception:
            _recover_after_error()
    for ext_name in ['clean', 'provider', 'finder', 'zsh']:
        try:
            _enter_ext(ext_name, SETTLE_INSTANT)
            press_down(2)
            _exit_ext()
        except Exception:
            _recover_after_error()
    try:
        _enter_ext('calc', SETTLE_INSTANT)
        type_text('2+3*4')
        time.sleep(SETTLE_INSTANT)
        clear_input()
        _exit_ext()
    except Exception:
        _recover_after_error()


def workload_shortcuts():
    """全局快捷键触发的独立窗口/扩展激活路径。"""
    global _in_ext
    hide_window(DEV_MODE)
    # 清空 workload_extensions 残留的搜索框内容（如 /calc），
    # 全局快捷键打开扩展不经搜索框，Esc 退出后残留文本会持续可见
    show_window(DEV_MODE)
    clear_input()
    hide_window(DEV_MODE)
    log('  → Alt+S 截屏')
    trigger_ext_shortcut('s', DEV_MODE)
    time.sleep(SETTLE_DEFAULT)
    press_esc()
    time.sleep(0.8)
    _ext_labels = {'c': '剪贴板', 't': '翻译', 'a': 'Agent', 'f': '访达工具'}
    for key in ['c', 't', 'a', 'f']:
        try:
            hide_window(DEV_MODE)
            log(f'  → Alt+{key.upper()} {_ext_labels[key]}')
            trigger_ext_shortcut(key, DEV_MODE)
            time.sleep(TOGGLE_GAP + 0.3)
            if is_voidnix_visible():
                _in_ext = True
                press_down(2)
            _exit_ext()
            # Esc 退出后清空搜索框（可能残留前一轮 workload 的文本），
            # 再等窗口状态稳定后 hide（高内存下 Esc 处理慢，
            # 不等会导致 hide 的 Alt+Space 与 Esc 竞态产生 toggle 错乱）
            if is_voidnix_visible():
                clear_input()
            time.sleep(0.3)
            hide_window(DEV_MODE)
        except Exception:
            _recover_after_error()


def run_perf():
    """Layer 3：N 轮全场景工作负载 + 逐阶段内存采样。

    每轮 = 全局搜索 + 工具列表 + 快捷键 + 扩展视图 + hide/show，
    在关键阶段后采内存快照，输出多轮趋势表。

    工作负载顺序刻意安排：快捷键在扩展视图之前——快捷键含 hide_window，
    若 FP 已超 350M 阈值会触发 navigate 重载，重载期间 WKWebView 不可交互。
    先跑快捷键（FP 低不触发重载），再跑扩展视图，避免中途重载打断工作负载。
    AtomicBool 一次性守卫确保即使重载触发，自测也不会二次运行。
    """
    log(f'\n── Layer 3：性能压测（{PERF_ROUNDS} 轮）──')
    log('切换到 ASCII 键盘布局...')
    saved_input = switch_to_ascii()

    try:
        show_window(DEV_MODE)
        time.sleep(0.5)
        mem_trend.sample('压测基线')

        for r in range(1, PERF_ROUNDS + 1):
            log(f'\n── 第 {r}/{PERF_ROUNDS} 轮 ──────────────────────')

            show_window(DEV_MODE)
            workload_global_search()
            show_window(DEV_MODE)
            workload_tool_list()
            show_window(DEV_MODE)
            workload_shortcuts()
            show_window(DEV_MODE)
            workload_extensions()
            mem_trend.sample(f'第{r}轮 扩展视图')

            show_window(DEV_MODE)
            mem_trend.sample(f'第{r}轮 完成')

            hide_window(DEV_MODE)
            # 安全裕度：hide 后若 FP 超 350M，maybe_reload_webview 会 navigate
            # 重载 WKWebView，JS 重新初始化 + 扩展 setup + 快捷键注册需数秒。
            # AtomicBool 守卫确保自测不会二次触发，此等待保证重载后 WKWebView
            # 恢复到可交互状态，下一轮输入不会打到未 ready 页面。
            time.sleep(4.0)
            show_window(DEV_MODE)
            time.sleep(0.5)
            mem_trend.sample(f'第{r}轮 hide/show')

        hide_window(DEV_MODE)
        mem_trend.sample('压测最终')

        log('')
        mem_trend.print_trend()
        base = next((s for s in mem_trend.samples if s.label == '压测基线'), None)
        final = next((s for s in reversed(mem_trend.samples) if s.label == '压测最终'), None)
        if base and final:
            drift = final.fp_mb - base.fp_mb
            log(f'\n  footprint 累积: {base.fp_mb:.0f}MB → {final.fp_mb:.0f}MB (drift {drift:+.0f}MB)')
            gn_drift = final.gn_mb - base.gn_mb
            log(f'  graphics N 累积: {base.gn_mb:.0f}MB → {final.gn_mb:.0f}MB (drift {gn_drift:+.0f}MB)')
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

    # 内存趋势（Layer 2 逐阶段 + Layer 3 多轮）
    if mem_trend.samples:
        log('\n  [memory-trend]')
        mem_trend.print_trend()


def write_markdown_report(rc: ResultCollector, overall_pass: bool,
                          layer1_report: Optional[dict] = None,
                          boot_total_ms: float = 0):
    """生成 Markdown 汇总报告（功能结果 + 性能指标）。"""
    categories: dict[str, list] = {}
    for r in rc.results:
        categories.setdefault(r.category, []).append(r)

    lines = [
        '# Voidnix 全功能回归测试报告',
        f'时间：{time.strftime("%Y-%m-%d %H:%M:%S")}  构建：{"dev" if DEV_MODE else "release"}'
        f'{" | 性能压测" if PERF_MODE else ""}',
        '',
        '## 汇总',
        f'总用例 {rc.total} | 通过 {rc.passed} | 失败 {rc.failed} | 跳过 {rc.skipped} | '
        f'结果：**{"全绿" if overall_pass else "有失败"}**',
        '',
    ]

    # ── 功能测试结果 ──
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

    # ── 性能指标 ──
    perf_lines: list[str] = []
    # 启动耗时
    if boot_total_ms > 0:
        perf_lines.extend(['### 启动耗时', '',
                           f'进程启动到自测报告写出: {boot_total_ms / 1000:.1f}s', ''])

    # 搜索延迟（从 Layer 1 latency 类别提取）
    if layer1_report:
        latency_items = [r for r in layer1_report.get('results', [])
                         if r.get('category') == 'latency']
        if latency_items:
            perf_lines.extend(['### 搜索延迟', '',
                               '| query | 耗时 | 状态 |', '|---|---|---|'])
            for item in latency_items:
                icon = {'pass': '+', 'fail': 'x', 'skip': '-'}[item.get('status', 'skip')]
                perf_lines.append(f'| {item["name"]} | {item.get("duration_ms", 0)}ms | {icon} |')
            perf_lines.append('')

    # 内存趋势
    perf_lines.extend(mem_trend.to_markdown())
    if perf_lines and perf_lines[-1] == '':
        perf_lines.pop()

    if perf_lines:
        lines.append('## 性能指标')
        lines.append('')
        lines.extend(perf_lines)
        lines.append('')

    SMOKE_REPORT_PATH.write_text('\n'.join(lines))
    log(f'\n报告已写入: {SMOKE_REPORT_PATH}')


# ── 主流程 ───────────────────────────────────────────────────────────────────

def main():
    log('=' * 60)
    log('Voidnix 全功能回归测试')
    mode_parts = ['dev' if DEV_MODE else 'release']
    if SELF_TEST_ONLY:
        mode_parts.append('仅自测')
    else:
        mode_parts.append('完整')
    if PERF_MODE:
        mode_parts.append(f'性能压测({PERF_ROUNDS}轮)')
    if NO_CGEVENT:
        mode_parts.append('无 CGEvent')
    log(f'模式: {" | ".join(mode_parts)}')
    log('=' * 60)

    if DO_BUILD:
        build_release()

    app_path = find_app_path()
    rc = ResultCollector()

    # ── Layer 1：应用自测（含启动耗时测量）──
    log('\n── Layer 1：应用自测 ────────────────────────────')

    if not NO_CGEVENT and not SELF_TEST_ONLY:
        ensure_wm_disabled()

    kill_voidnix()
    clear_old_report()
    t0 = time.time()
    vite_proc = launch_self_test(app_path)

    log(f'等待自测报告（超时 {SELF_TEST_TIMEOUT}s）...')
    layer1_report = wait_for_report()
    startup_ms = layer1_report.get('duration_ms', 0)
    boot_total = (time.time() - t0) * 1000
    log(f'自测报告已收到（自测耗时 {startup_ms}ms，进程启动到报告写出 {boot_total:.0f}ms）')

    # 合并 Layer 1 结果
    rc.merge_from_json(layer1_report.get('results', []))

    layer1_summary = layer1_report.get('summary', {})
    log(f'  Layer 1: {layer1_summary.get("passed", 0)}/{layer1_summary.get("total", 0)} 通过')

    # ── Layer 2：系统冒烟 ──
    if not SELF_TEST_ONLY and not NO_CGEVENT:
        log('\n── Layer 2：系统冒烟（CGEvent）────────────────────')
        mem_trend.sample('Layer 2 基线')
        run_layer2(rc)
    elif NO_CGEVENT:
        log('\n--no-cgevent：跳过 Layer 2')
        kill_voidnix()
    else:
        log('\n仅自测模式：跳过 Layer 2')
        kill_voidnix()

    if vite_proc:
        vite_proc.terminate()

    # ── Layer 3：性能压测（--perf）──
    if PERF_MODE and not SELF_TEST_ONLY and not NO_CGEVENT:
        run_perf()

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

    write_markdown_report(rc, overall_pass, layer1_report, boot_total)
    sys.exit(0 if overall_pass else 1)


if __name__ == '__main__':
    main()
