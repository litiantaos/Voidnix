// ═══════════════════════════════════════════════
//  播放器——播放循环 + 键盘控制 + UI 交互 + 自适应缩放
//  createPlayer({ renderFrame, $, controls }) 完成全部接线
//  controls=false 时自动连续播放，跳过控制栏 DOM 接线
// ═══════════════════════════════════════════════
import { FPS, SEGMENTS, SEG_OFFSETS, globalToSeg, clamp } from './demo-utils'
import { getDemoText, type DemoText } from '../../i18n/demo'
import type { Lang } from '../../i18n/translations'
import type { Renderer } from './demo-scenes'

type El = HTMLElement
type Getter = (id: string) => El | null

interface PlayerOpts {
  renderer: Renderer
  $: Getter
  controls?: boolean
}

export function createPlayer({ renderer, $, controls = true }: PlayerOpts) {
  const { renderFrame } = renderer
  const stageEl = $('stage')!
  const stageFrame = $('stageFrame')!

  // 语言文案
  const langEl = document.querySelector('.demo-stage') as HTMLElement | null
  const lang = (langEl?.dataset.lang as Lang) || 'zh'
  const T: DemoText = getDemoText(lang)

  // ── 自适应缩放 ──
  function fitStage() {
    const wrap = stageEl.closest('.demo-stage') as HTMLElement | null
    if (!wrap) return
    const w = wrap.clientWidth
    const scale = w / 1280
    stageEl.style.transform = `scale(${scale})`
    stageFrame.style.height = `${720 * scale}px`
  }
  new ResizeObserver(fitStage).observe(stageEl.closest('.demo-stage') as HTMLElement)
  fitStage()

  // ── 播放状态（无控制栏时默认连续播放）──
  let playMode: 'single' | 'all' = controls ? 'single' : 'all'
  let currentSeg = 0
  let baseTime = performance.now()
  let paused = false
  let pauseAt = 0
  let frameOverride = -1

  function computeState(): { segIdx: number; lf: number; progress: number } {
    let f: number
    if (frameOverride >= 0) {
      f = frameOverride
    } else {
      const now = paused ? pauseAt : performance.now()
      f = Math.floor(((now - baseTime) / 1000) * FPS)
    }

    if (playMode === 'single') {
      const seg = SEGMENTS[currentSeg]
      const lf = ((f % seg.dur) + seg.dur) % seg.dur
      return { segIdx: currentSeg, lf, progress: lf / seg.dur }
    }
    const { segIdx, lf } = globalToSeg(f)
    return { segIdx, lf, progress: lf / SEGMENTS[segIdx].dur }
  }

  // ── 模式判断 ──
  const params = new URLSearchParams(location.search)
  const isCapture = params.get('capture') === '1'
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

  // ── 捕获模式 ──
  window.__renderFrame = async (globalFrame: number) => {
    const { segIdx, lf } = globalToSeg(globalFrame)
    renderFrame(segIdx, lf)
    await new Promise((r) => requestAnimationFrame(r))
  }

  if (isCapture) {
    renderFrame(0, 0)
  } else if (reducedMotion) {
    renderFrame(0, 104)
  } else if (controls) {
    // ── 有控制栏：完整 UI 交互 ──
    const segBtns = $('segBtns')!
    const allBtn = $('allBtn')!
    const playBtn = $('playBtn')!
    const progFill = $('progFill')!

    let lastSegIdx = -1,
      lastLf = -1
    function loop() {
      const { segIdx, lf, progress } = computeState()
      if (segIdx !== lastSegIdx || lf !== lastLf) {
        renderFrame(segIdx, lf)
        lastSegIdx = segIdx
        lastLf = lf
        progFill.style.width = `${progress * 100}%`
        updateActiveBtn(segIdx)
      }
      requestAnimationFrame(loop)
    }
    loop()

    function togglePause() {
      if (paused) {
        // 恢复时以当前帧（frameOverride 或时间推算）为起点重设 baseTime
        const targetFrame =
          frameOverride >= 0 ? frameOverride : Math.floor(((pauseAt - baseTime) / 1000) * FPS)
        baseTime = performance.now() - (targetFrame / FPS) * 1000
        frameOverride = -1
        paused = false
      } else {
        pauseAt = performance.now()
        paused = true
      }
      updatePlayIcon()
    }

    function updatePlayIcon() {
      playBtn.innerHTML = paused ? '<i class="ri-play-fill"></i>' : '<i class="ri-pause-fill"></i>'
    }

    document.addEventListener('keydown', (e) => {
      if (e.target instanceof HTMLButtonElement) return
      // 仅舞台在视口内接管空格/方向键，滚出视口后交还页面默认行为
      const rect = stageFrame.getBoundingClientRect()
      if (rect.bottom < 0 || rect.top > window.innerHeight) return
      if (e.key === ' ') {
        e.preventDefault()
        togglePause()
      } else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
        e.preventDefault()
        if (!paused) {
          paused = true
          pauseAt = performance.now()
          updatePlayIcon()
        }
        const cur =
          frameOverride >= 0 ? frameOverride : Math.floor(((pauseAt - baseTime) / 1000) * FPS)
        frameOverride = cur + (e.key === 'ArrowRight' ? 1 : -1)
      }
    })

    playBtn.addEventListener('click', togglePause)

    // ── 进度条拖动 ──
    const progTrack = $('progTrack')!
    let dragging = false

    function seekToProgress(p: number) {
      const segIdx = playMode === 'single' ? currentSeg : Math.max(0, lastSegIdx)
      const seg = SEGMENTS[segIdx]
      const targetLf = Math.round(clamp(p, 0, 1) * seg.dur)
      if (!paused) {
        paused = true
        pauseAt = performance.now()
        updatePlayIcon()
      }
      frameOverride = playMode === 'single' ? targetLf : SEG_OFFSETS[segIdx] + targetLf
    }

    function clientXToProgress(clientX: number): number {
      const rect = progTrack.getBoundingClientRect()
      return clamp((clientX - rect.left) / rect.width, 0, 1)
    }

    progTrack.addEventListener('pointerdown', (e) => {
      e.preventDefault()
      dragging = true
      progTrack.classList.add('dragging')
      progTrack.setPointerCapture(e.pointerId)
      seekToProgress(clientXToProgress(e.clientX))
    })
    progTrack.addEventListener('pointermove', (e) => {
      if (!dragging) return
      seekToProgress(clientXToProgress(e.clientX))
    })
    progTrack.addEventListener('pointerup', (e) => {
      if (!dragging) return
      dragging = false
      progTrack.classList.remove('dragging')
      progTrack.releasePointerCapture(e.pointerId)
    })

    segBtns.querySelectorAll('.seg-btn').forEach((btn) => {
      btn.addEventListener('click', () => {
        const el = btn as HTMLElement
        currentSeg = parseInt(el.dataset.i!)
        playMode = 'single'
        baseTime = performance.now()
        frameOverride = -1
        paused = false
        updatePlayIcon()
        allBtn.classList.remove('active')
        allBtn.textContent = T.btnPlayAll
      })
    })

    allBtn.addEventListener('click', () => {
      if (playMode === 'all') {
        playMode = 'single'
        allBtn.classList.remove('active')
        allBtn.textContent = T.btnPlayAll
        baseTime = performance.now()
      } else {
        playMode = 'all'
        allBtn.classList.add('active')
        allBtn.textContent = T.btnPlaySeg
        // 从当前选中段开始（全局帧 = 该段起始偏移）
        baseTime = performance.now() - (SEG_OFFSETS[currentSeg] / FPS) * 1000
      }
      frameOverride = -1
      paused = false
      updatePlayIcon()
    })
  } else {
    // ── 无控制栏（首页）：纯连续播放循环 ──
    let lastSegIdx = -1,
      lastLf = -1
    function loop() {
      const { segIdx, lf } = computeState()
      if (segIdx !== lastSegIdx || lf !== lastLf) {
        renderFrame(segIdx, lf)
        lastSegIdx = segIdx
        lastLf = lf
      }
      requestAnimationFrame(loop)
    }
    loop()
  }
}

function updateActiveBtn(segIdx: number) {
  const segBtns = document.getElementById('segBtns')
  if (!segBtns) return
  segBtns.querySelectorAll('.seg-btn').forEach((btn) => {
    const el = btn as HTMLElement
    if (parseInt(el.dataset.i!) === segIdx) btn.classList.add('active')
    else btn.classList.remove('active')
  })
}
