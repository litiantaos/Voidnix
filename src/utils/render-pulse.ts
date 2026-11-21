import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { isTauri } from '@/utils/tauri'

/** 上次渲染自愈重载的时间戳（sessionStorage 同浏览会话跨 reload 存活，与激活
 *  扩展恢复同存储语义）。10 分钟老化：防循环只需拦「reload 后立即仍冻结」的
 *  短窗口；数小时后新的空白（必然伴随新的唤起 + 用户在场）应允许再次自愈。 */
const REVIVE_RELOADED_KEY = 'voidnix:render-revive-reloaded'
const REVIVE_RELOAD_COOLDOWN_MS = 10 * 60 * 1000

/** 用户在场证据窗口：真空白场景用户必然伴随按键（打字 / Esc / 热键关窗后的
 *  再唤起）；而息屏 / 锁屏 / 快速用户切换时窗口逻辑可见但整屏 occluded，rAF
 *  停摆与冻结 bug 在 JS 视角完全同构（visibilityState 均 hidden），唯一区分
 *  是零交互——reload 由此门槛挡住。 */
const PRESENCE_WINDOW_MS = 60 * 1000

let lastKeydownAt = 0
let keydownArmed = false

/** 惰性挂载 keydown 时间戳监听（capture + passive，零开销；非 Tauri 永不挂）。 */
function armKeydownPresence() {
  if (keydownArmed) return
  keydownArmed = true
  document.addEventListener(
    'keydown',
    () => {
      lastKeydownAt = Date.now()
    },
    { capture: true, passive: true },
  )
}

/** 探测渲染时钟：`ms` 内 rAF 是否执行。
 *  WKWebView 判窗口不可见时冻结渲染时钟——只停摆 rAF / CSS 动画，JS 事件循环与
 *  timer 照跑（本项目隐藏窗口行为的既有实证）。timer 到点仍未见证 rAF 即冻结。 */
function probeRenderPulse(ms: number): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false
    const timer = setTimeout(() => {
      if (!settled) {
        settled = true
        resolve(false)
      }
    }, ms)
    requestAnimationFrame(() => {
      if (!settled) {
        settled = true
        clearTimeout(timer)
        resolve(true)
      }
    })
  })
}

let healing = false

/** 渲染时钟健康检测 + 两级自愈（唤出画面空白时自动恢复）。
 *
 *  根因（开机自启早期偶发）：登录风暴期 WindowServer/WebKit 竞态下，「从未
 *  orderFront → show」的可见性翻转不被 WebKit occlusion 感知——WebContent 存活、
 *  JS/IPC 正常，但渲染时钟永久停摆：画面空白、hide/show 不重建渲染管线、重启
 *  app 才恢复（监控实证：WebContent 停在正常渲染水位的一半，合成资源从未分配）。
 *
 *  检测：可见窗口的 rAF 应在数十 ms 内执行，2s 未执行 = 冻结。正常路径开销仅
 *  一次立即通过的 rAF 探测（~16ms）。L1：Rust kick（setNeedsDisplay + alpha
 *  微扰 + orderFront 重发，重算 occlusion）；L2：仍冻结则 location.reload()
 *  重建渲染管线。reload 三道闸：窗口仍可见（探测期间正常关窗的冻结是设计行为）、
 *  用户在场（防息屏 / 锁屏误判，见 PRESENCE_WINDOW_MS）、冷却期已过（防循环）。
 *  in-flight 期间重入直跳（幂等）。 */
export async function ensureRenderPulse() {
  if (!isTauri || healing) return
  healing = true
  armKeydownPresence()
  try {
    if (await probeRenderPulse(2000)) return
    await invoke(CMD.kickWebviewRender).catch(() => {})
    if (await probeRenderPulse(2000)) return
    if (!(await invoke<boolean>(CMD.isMainWindowVisible).catch(() => true))) return
    if (Date.now() - lastKeydownAt > PRESENCE_WINDOW_MS) return
    const lastReload = Number(sessionStorage.getItem(REVIVE_RELOADED_KEY)) || 0
    if (Date.now() - lastReload < REVIVE_RELOAD_COOLDOWN_MS) return
    sessionStorage.setItem(REVIVE_RELOADED_KEY, String(Date.now()))
    location.reload()
  } finally {
    healing = false
  }
}
