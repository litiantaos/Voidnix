// ═══════════════════════════════════════════════
//  场景渲染器工厂——6 段动画 + 辅助函数
//  createRenderer($) 返回 { renderFrame, resetStage }
// ═══════════════════════════════════════════════
import {
  SEGMENTS,
  SEG_DIS,
  KBD,
  KBD_END,
  APP,
  APP_END,
  type SegCtx,
  clamp,
  lerp,
  easeOut,
  easeInOut,
  spring,
  typeSlice,
} from './demo-utils'

type El = HTMLElement
type Getter = (id: string) => El | null

export interface Renderer {
  renderFrame: (segIdx: number, f: number) => void
  resetStage: () => void
}

export function createRenderer($: Getter): Renderer {
  // ── 元素引用 ──
  const fogA = $('fogA')!,
    fogB = $('fogB')!
  const launcher = $('launcher')!
  const extTag = $('extTag')!,
    extTagIcon = $('extTagIcon')!,
    extTagName = $('extTagName')!
  const searchText = $('searchText')!,
    searchCursor = $('searchCursor')!
  const panelSearch = $('panelSearch')!,
    panelClipboard = $('panelClipboard')!,
    panelAgent = $('panelAgent')!
  const grpApp = $('grpApp')!,
    grpFile = $('grpFile')!
  const rowVSCode = $('rowVSCode')!,
    rowXcode = $('rowXcode')!
  const rowVoidnix = $('rowVoidnix')!,
    rowCodegen = $('rowCodegen')!,
    rowConfig = $('rowConfig')!
  const titleVSCode = $('titleVSCode')!
  const clip1 = $('clip1')!,
    clip2 = $('clip2')!,
    clip3 = $('clip3')!,
    clip4 = $('clip4')!,
    clip5 = $('clip5')!
  const agentUserRow = $('agentUserRow')!,
    agentUserBubble = $('agentUserBubble')!
  const agentCard = $('agentCard')!,
    agentTool = $('agentTool')!,
    agentResult = $('agentResult')!
  const agentFooter = $('agentFooter')!
  const shotOverlay = $('shotOverlay')!,
    shotSel = $('shotSel')!,
    shotFlash = $('shotFlash')!,
    shotToolbar = $('shotToolbar')!
  const shotOcrBtn = $('shotOcrBtn')!,
    shotScrollBtn = $('shotScrollBtn')!,
    shotScrollPreview = $('shotScrollPreview')!,
    sspContent = $('sspContent')!
  const shotRect = $('shotRect')!,
    shotMag = $('shotMag')!
  const shotRectBtn = $('shotRectBtn')!
  const cursorCross = $('cursorCross')!
  const awCodeViewport = $('awCodeViewport')!
  const panelOcr = $('panelOcr')!,
    ocrPreview = $('ocrPreview')!,
    ocrTextArea = $('ocrTextArea')!,
    ocrAction = $('ocrAction')!
  const winVscode = $('winVscode')!,
    winTerm = $('winTerm')!
  const cursorEl = $('cursor')!,
    snapPanel = $('snapPanel')!,
    snapTarget = $('snapTarget')!
  const finderWin = $('finderWin')!,
    finderPanel = $('finderPanel')!
  const kbdPop = $('kbdPop')!,
    captionEl = $('caption')!
  const agentText = $('agentText')!,
    demoToast = $('demoToast')!
  const editorWin = $('editorWin')!,
    ewPasted = $('ewPasted')!,
    ewCursor = $('ewCursor')!

  // --cool 基元
  const COOL_RGB =
    getComputedStyle(document.documentElement).getPropertyValue('--cool').trim() || '100 120 160'

  // ── 段上下文 ──
  let _segCtx: SegCtx = { dur: 160, dis: 132 }

  // ── 脏检查状态 ──
  let _lastSeg = -1,
    _kbdOn = false,
    _kbdHtml = '',
    _capText = '',
    _capOp = -1,
    _sbText = ''

  // ═══════════════════════════════════════════════
  //  段切换时重置全部元素到隐藏态
  // ═══════════════════════════════════════════════
  function resetStage() {
    launcher.style.opacity = '0'
    panelSearch.style.opacity = '0'
    panelClipboard.style.opacity = '0'
    panelAgent.style.opacity = '0'
    extTag.style.display = 'none'
    searchText.textContent = ''
    searchCursor.style.opacity = '0'
    const allItems: El[] = [
      grpApp,
      rowVSCode,
      rowXcode,
      grpFile,
      rowVoidnix,
      rowCodegen,
      rowConfig,
      clip1,
      clip2,
      clip3,
      clip4,
      clip5,
      agentUserRow,
      agentCard,
      agentFooter,
    ]
    for (const el of allItems) {
      el.style.opacity = '0'
      el.style.transform = ''
    }
    rowVSCode.classList.remove('selected')
    rowVSCode.style.background = ''
    titleVSCode.style.color = ''
    titleVSCode.style.fontWeight = ''
    clip1.classList.remove('selected')
    clip1.style.background = ''
    clip2.classList.remove('selected')
    clip2.style.background = ''
    agentText.style.opacity = '0'
    agentText.textContent = ''
    demoToast.style.opacity = '0'
    snapTarget.classList.remove('sp-hover')
    editorWin.style.opacity = '0'
    ewPasted.textContent = ''
    ewPasted.style.opacity = '0'
    ewCursor.style.opacity = '0'
    agentTool.style.opacity = '0'
    agentResult.style.opacity = '0'
    agentUserBubble.textContent = ''
    shotOverlay.style.opacity = '0'
    shotToolbar.style.opacity = '0'
    shotFlash.style.opacity = '0'
    shotSel.style.transform = ''
    shotSel.style.width = ''
    shotSel.style.height = ''
    shotSel.style.left = ''
    shotSel.style.top = ''
    shotSel.style.opacity = ''
    shotOcrBtn.classList.remove('active')
    shotScrollBtn.classList.remove('active')
    shotScrollPreview.style.opacity = '0'
    shotRect.style.opacity = '0'
    shotRect.style.width = '0'
    shotRect.style.height = '0'
    shotRectBtn.classList.remove('active')
    shotMag.style.opacity = '0'
    cursorCross.style.opacity = '0'
    awCodeViewport.style.transform = ''
    sspContent.style.transform = ''
    shotToolbar.style.left = ''
    shotToolbar.style.top = ''
    panelOcr.style.opacity = '0'
    panelOcr.style.transform = ''
    ocrTextArea.textContent = ''
    winVscode.style.opacity = '0'
    winTerm.style.opacity = '0'
    winVscode.style.left = ''
    winVscode.style.top = ''
    winVscode.style.width = ''
    winVscode.style.height = ''
    winTerm.style.left = ''
    winTerm.style.top = ''
    winTerm.style.width = ''
    winTerm.style.height = ''
    cursorEl.style.opacity = '0'
    snapPanel.style.opacity = '0'
    finderWin.style.opacity = '0'
    finderPanel.style.opacity = '0'
    kbdPop.style.opacity = '0'
    captionEl.style.opacity = '0'
  }

  // ═══════════════════════════════════════════════
  //  雾团
  // ═══════════════════════════════════════════════
  function updateFog(f: number) {
    const t = (f / _segCtx.dur) * Math.PI * 2
    fogA.style.transform = `translate(${Math.sin(t) * 3}%, ${Math.cos(t * 0.7) * 2}%) scale(${1 + Math.sin(t) * 0.03})`
    fogA.style.opacity = String(0.7 + Math.sin(t * 0.5) * 0.08)
    fogB.style.transform = `translate(${-Math.sin(t) * 4}%, ${-Math.cos(t * 0.6) * 3}%) scale(${1 + Math.cos(t) * 0.04})`
    fogB.style.opacity = String(0.55 + Math.cos(t * 0.4) * 0.1)
  }

  // ═══════════════════════════════════════════════
  //  快捷键键帽
  // ═══════════════════════════════════════════════
  function renderKbd(f: number, opt: string, main: string, start = KBD) {
    const lf = f - start
    if (lf < 0 || lf > KBD_END - KBD) {
      if (_kbdOn) {
        kbdPop.style.opacity = '0'
        _kbdOn = false
      }
      return
    }
    _kbdOn = true
    const enter = spring(lf, 0, 130, 16)
    const fade = lf > 12 ? 1 - clamp((lf - 12) / 8, 0, 1) : 1
    const html = `<span class="kbd-opt">${opt}</span><span class="kbd-main">${main}</span>`
    if (html !== _kbdHtml) {
      kbdPop.innerHTML = html
      _kbdHtml = html
    }
    kbdPop.style.opacity = String(fade)
    kbdPop.style.transform = `translate(-50%, -50%) scale(${0.5 + enter * 0.5})`
  }

  // ═══════════════════════════════════════════════
  //  字幕
  // ═══════════════════════════════════════════════
  function renderCaption(segIdx: number, f: number) {
    const seg = SEGMENTS[segIdx]
    const dis = _segCtx.dis
    const text = f >= 32 && f < dis ? seg.cap : ''
    const fadeIn = clamp((f - 34) / 8, 0, 1)
    const fadeOut = f > dis - 10 ? 1 - clamp((f - dis + 10) / 10, 0, 1) : 1
    const op = text ? fadeIn * fadeOut : 0
    if (text && text !== _capText) {
      captionEl.textContent = text
      _capText = text
    }
    if (op !== _capOp) {
      captionEl.style.opacity = String(op)
      _capOp = op
    }
  }

  // ═══════════════════════════════════════════════
  //  启动器出现/消失辅助
  // ═══════════════════════════════════════════════
  function launcherState(f: number, dismissStart?: number, dismissDur = 24) {
    const ds = dismissStart ?? _segCtx.dis
    const appear = clamp((f - APP) / (APP_END - APP), 0, 1)
    const dismiss = f > ds ? 1 - clamp((f - ds) / dismissDur, 0, 1) : 1
    const op = appear * dismiss
    const s = spring(f, APP, 100, 16)
    return { op, scale: 0.94 + s * 0.06, blur: (1 - appear) * 10 }
  }

  function applyLauncher(f: number, dismissStart?: number, dismissDur?: number): number {
    const { op, scale, blur } = launcherState(f, dismissStart, dismissDur)
    launcher.style.opacity = String(op)
    launcher.style.transform = `scale(${scale}) translateY(${(1 - spring(f, APP, 100, 16)) * 8}px)`
    launcher.style.filter = blur > 0.1 ? `blur(${blur}px)` : 'none'
    return op
  }

  function setSbText(text: string, color: string) {
    if (text !== _sbText) {
      searchText.textContent = text
      searchText.style.color = color
      _sbText = text
    }
  }

  // ═══════════════════════════════════════════════
  //  段：搜索
  // ═══════════════════════════════════════════════
  const SEARCH_ROWS: [El, number][] = [
    [grpApp, 70],
    [rowVSCode, 73],
    [rowXcode, 76],
    [grpFile, 79],
    [rowVoidnix, 82],
    [rowCodegen, 85],
    [rowConfig, 88],
  ]
  function renderSearch(f: number) {
    renderKbd(f, '⌥', 'Space')
    const op = applyLauncher(f)
    if (f < 54) setSbText('搜索应用、文件、扩展等', 'var(--color-text-muted)')
    else if (f <= 72) setSbText(typeSlice(f, 54, 72, 'code'), 'var(--color-text-primary)')
    else setSbText('code', 'var(--color-text-primary)')

    if (f >= 54 && f <= 72) searchCursor.style.opacity = '1'
    else if (f > 72 && f < _segCtx.dis)
      searchCursor.style.opacity = Math.floor(f / 15) % 2 === 0 ? '1' : '0'
    else searchCursor.style.opacity = '0'

    panelSearch.style.opacity = String(op)
    panelSearch.style.transform = `translateY(${(1 - clamp((f - 68) / 10, 0, 1)) * 6}px)`
    for (const [el, enter] of SEARCH_ROWS) {
      const s = spring(f, enter, 100, 16)
      el.style.opacity = String(s * op)
      el.style.transform = `translateY(${(1 - s) * 10}px)`
    }
    const active = clamp((f - 96) / 8, 0, 1)
    if (active > 0.01) {
      rowVSCode.classList.add('selected')
      rowVSCode.style.background = `rgb(${COOL_RGB} / ${active * 0.11})`
      titleVSCode.style.color = 'var(--color-accent)'
      titleVSCode.style.fontWeight = active > 0.5 ? '500' : '400'
    } else {
      rowVSCode.classList.remove('selected')
      rowVSCode.style.background = 'transparent'
      titleVSCode.style.color = 'var(--color-text-primary)'
      titleVSCode.style.fontWeight = '400'
    }
  }

  // ═══════════════════════════════════════════════
  //  段：剪贴板
  // ═══════════════════════════════════════════════
  const CLIP_ROWS: [El, number][] = [
    [clip1, 56],
    [clip2, 62],
    [clip3, 68],
    [clip4, 74],
    [clip5, 80],
  ]
  function renderClipboard(f: number) {
    renderKbd(f, '⌥', 'C')
    const op = applyLauncher(f, 116, 12)

    const edIn = f >= 126 ? clamp((f - 126) / 4, 0, 1) : 0
    const edOut = f > 170 ? 1 - clamp((f - 170) / 10, 0, 1) : 1
    editorWin.style.opacity = String(edIn * edOut)

    extTagIcon.className = 'ext-tag-icon ri-clipboard-line'
    extTagName.textContent = '剪贴板'
    extTag.style.display = 'flex'
    const tagOp = clamp((f - APP) / 10, 0, 1) * clamp(1 - (f > 116 ? (f - 116) / 8 : 0), 0, 1)
    extTag.style.opacity = String(tagOp)

    setSbText('在 剪贴板 中搜索', 'var(--color-text-muted)')
    searchCursor.style.opacity = '0'

    panelClipboard.style.opacity = String(op)
    panelClipboard.style.transform = `translateY(${(1 - clamp((f - 54) / 10, 0, 1)) * 6}px)`
    for (const [el, enter] of CLIP_ROWS) {
      const s = spring(f, enter, 100, 16)
      el.style.opacity = String(s * op)
      el.style.transform = `translateY(${(1 - s) * 10}px)`
    }
    const active = clamp((f - 88) / 8, 0, 1)
    clip1.style.background = active > 0.01 ? `rgb(${COOL_RGB} / ${active * 0.11})` : 'transparent'

    if (f >= 134) {
      ewPasted.style.opacity = String(clamp((f - 134) / 4, 0, 1))
      ewPasted.textContent = 'const FPS = 30'
      ewCursor.style.opacity = Math.floor(f / 15) % 2 === 0 ? '1' : '0'
    } else {
      ewPasted.style.opacity = '0'
      ewPasted.textContent = ''
      ewCursor.style.opacity = '0'
    }
    if (f >= 138) {
      const tin = clamp((f - 138) / 6, 0, 1)
      const tout = f > 170 ? 1 - clamp((f - 170) / 10, 0, 1) : 1
      demoToast.style.opacity = String(tin * tout)
    } else {
      demoToast.style.opacity = '0'
    }
  }

  // ═══════════════════════════════════════════════
  //  段：Agent
  // ═══════════════════════════════════════════════
  function renderAgent(f: number) {
    renderKbd(f, '⌥', 'A')
    const op = applyLauncher(f, 168)

    extTagIcon.className = 'ext-tag-icon ri-robot-2-line'
    extTagName.textContent = 'Agent'
    extTag.style.display = 'flex'
    const tagOp = clamp((f - APP) / 10, 0, 1) * clamp(1 - (f > 168 ? (f - 168) / 8 : 0), 0, 1)
    extTag.style.opacity = String(tagOp)

    setSbText('在 Agent 中搜索', 'var(--color-text-muted)')
    searchCursor.style.opacity = '0'

    panelAgent.style.opacity = String(op)
    panelAgent.style.transform = `translateY(${(1 - clamp((f - 54) / 10, 0, 1)) * 6}px)`

    agentUserRow.style.opacity = String(spring(f, 54, 100, 16) * op)
    agentUserBubble.textContent = '列出 extensions 目录下的 ts 文件'

    const toolS = spring(f, 66, 100, 16)
    agentCard.style.opacity = String(toolS * op)
    agentTool.style.opacity = String(toolS * op)
    agentResult.style.opacity = String(spring(f, 80, 100, 16) * op)

    const textS = spring(f, 92, 100, 16)
    agentText.style.opacity = String(textS * op)
    agentText.textContent = typeSlice(
      f,
      92,
      132,
      '找到 23 个 index.ts，含 native 的 16 个。\n\n这些文件分布在 extensions/ 下的每个扩展目录中，纯 TS 扩展（calculator、ip 等）同样包含 index.ts。还需要其他帮助吗？',
    )

    const footerS = clamp((f - APP_END) / 8, 0, 1) * clamp(1 - (f > 168 ? (f - 168) / 8 : 0), 0, 1)
    agentFooter.style.opacity = String(footerS)
  }

  // ═══════════════════════════════════════════════
  //  段：截屏（标注 → 滚动截屏 → OCR）
  //
  //  A. 标注 (0-120)
  //  B. 滚动截屏 (120-215)
  //  C. OCR (215-350)
  // ═══════════════════════════════════════════════
  function renderShot(f: number) {
    renderKbd(f, '⌥', 'S')
    const dis = _segCtx.dis

    // ── A: 标注阶段 (0-120) ──
    if (f < 120) {
      const ovIn = clamp(f / 14, 0, 1)
      shotOverlay.style.opacity = String(ovIn)
      winVscode.style.opacity = String(ovIn)
      winTerm.style.opacity = String(ovIn * 0.96)
      shotFlash.style.opacity = '0'

      // 字幕
      setShotCaption(f, 0, '截屏标注：拉出选区 + 标注工具')

      const SEL_X = 160,
        SEL_Y = 120,
        SEL_W = 960,
        SEL_H = 480
      const dragStart = 28,
        dragEnd = 56

      if (f < dragStart) {
        shotSel.style.opacity = '0'
        shotSel.style.width = '0px'
        shotSel.style.height = '0px'
        shotSel.style.left = SEL_X + 'px'
        shotSel.style.top = SEL_Y + 'px'
      } else {
        shotSel.style.opacity = '1'
        const dragE = easeInOut(clamp((f - dragStart) / (dragEnd - dragStart), 0, 1))
        shotSel.style.width = lerp(0, SEL_W, dragE) + 'px'
        shotSel.style.height = lerp(0, SEL_H, dragE) + 'px'
        shotSel.style.left = SEL_X + 'px'
        shotSel.style.top = SEL_Y + 'px'
      }

      // 十字标 + 放大窗
      if (f >= dragStart && f < 60) {
        const dragE = easeInOut(clamp((f - dragStart) / (dragEnd - dragStart), 0, 1))
        const cx = SEL_X + lerp(0, SEL_W, dragE)
        const cy = SEL_Y + lerp(0, SEL_H, dragE)
        const curOp =
          clamp((f - dragStart) / 4, 0, 1) * (f > 52 ? 1 - clamp((f - 52) / 8, 0, 1) : 1)
        cursorCross.style.opacity = String(curOp)
        cursorCross.style.transform = `translate(${cx - 10}px, ${cy - 10}px)`
        const magOp =
          clamp((f - dragStart) / 4, 0, 1) * (f > 50 ? 1 - clamp((f - 50) / 10, 0, 1) : 1)
        shotMag.style.opacity = String(magOp)
        shotMag.style.transform = `translate(${cx + 20}px, ${cy - 105}px)`
      } else {
        cursorCross.style.opacity = '0'
        shotMag.style.opacity = '0'
      }

      // 工具栏固定在选区底部左侧
      shotToolbar.style.opacity = String(clamp((f - 56) / 8, 0, 1))
      shotToolbar.style.left = SEL_X + 'px'
      shotToolbar.style.top = SEL_Y + SEL_H + 8 + 'px'
      shotRectBtn.classList.toggle('active', f >= 66)
      shotOcrBtn.classList.remove('active')
      shotScrollBtn.classList.remove('active')

      const rectDrag = easeOut(clamp((f - 82) / 26, 0, 1))
      if (f >= 82 && f < 120) {
        shotRect.style.opacity = String(clamp((f - 82) / 4, 0, 1))
        shotRect.style.width = lerp(0, 150, rectDrag) + 'px'
        shotRect.style.height = lerp(0, 130, rectDrag) + 'px'
      } else if (f < 82) {
        shotRect.style.opacity = '0'
        shotRect.style.width = '0'
        shotRect.style.height = '0'
      }

      shotScrollPreview.style.opacity = '0'
      launcher.style.opacity = '0'
      awCodeViewport.style.transform = ''
    }

    // ── B: 滚动截屏 (120-215) ──
    else if (f < 215) {
      shotOverlay.style.opacity = '1'
      shotRectBtn.classList.remove('active')
      shotOcrBtn.classList.remove('active')

      // 字幕
      setShotCaption(f, 120, '滚动截屏：长内容连续捕获')

      // 红框消失
      const rectOut = clamp(1 - (f - 120) / 8, 0, 1)
      shotRect.style.opacity = String(rectOut)

      // 选区缩小移到 VS Code 代码区
      const SEL_X2 = 420,
        SEL_Y2 = 183,
        SEL_W2 = 430,
        SEL_H2 = 327
      const moveE = easeInOut(clamp((f - 120) / 16, 0, 1))
      const curSelX = lerp(160, SEL_X2, moveE)
      const curSelY = lerp(120, SEL_Y2, moveE)
      const curSelW = lerp(960, SEL_W2, moveE)
      const curSelH = lerp(480, SEL_H2, moveE)
      shotSel.style.opacity = '1'
      shotSel.style.left = curSelX + 'px'
      shotSel.style.top = curSelY + 'px'
      shotSel.style.width = curSelW + 'px'
      shotSel.style.height = curSelH + 'px'

      // 工具栏跟随选区底部左侧
      shotToolbar.style.opacity = '1'
      shotToolbar.style.left = curSelX + 'px'
      shotToolbar.style.top = curSelY + curSelH + 8 + 'px'

      // 滚动截屏按钮高亮（132-195）
      shotScrollBtn.classList.toggle('active', f >= 132 && f < 195)

      // 代码区滚动 + 右侧预览面板增长（140-205）
      if (f >= 140) {
        const prevIn = clamp((f - 140) / 10, 0, 1)
        const growE = easeOut(clamp((f - 150) / 40, 0, 1))
        const prevOut = f > 195 ? 1 - clamp((f - 195) / 10, 0, 1) : 1
        shotScrollPreview.style.opacity = String(prevIn * prevOut)
        shotScrollPreview.style.height = lerp(100, 480, growE) + 'px'

        // VS Code 代码区整体上移模拟滚动
        const scrollE = easeInOut(clamp((f - 150) / 45, 0, 1))
        awCodeViewport.style.transform = `translateY(-${lerp(0, 180, scrollE)}px)`
      } else {
        shotScrollPreview.style.opacity = '0'
        awCodeViewport.style.transform = ''
      }

      cursorCross.style.opacity = '0'
      shotMag.style.opacity = '0'
      launcher.style.opacity = '0'
    }

    // ── C: OCR (215-340) ──
    else {
      shotScrollBtn.classList.remove('active')
      shotScrollPreview.style.opacity = '0'

      // 字幕
      setShotCaption(f, 225, '截图 OCR：文字识别与复制')

      // 滚动截屏结束后留 10f 停顿（215-225），再激活 OCR 按钮
      shotOcrBtn.classList.toggle('active', f >= 225 && f < 238)

      if (f < 238) {
        shotOverlay.style.opacity = '1'
        shotToolbar.style.opacity = '1'
        shotSel.style.opacity = '1'
        shotRect.style.opacity = '0'
      }

      // overlay 关闭 → 启动器出现（238+），用 launcher 弹簧入场
      if (f >= 238) {
        shotOcrBtn.classList.remove('active')
        // overlay 先快速消失（238-246）
        const ovOut = f < 246 ? 1 - clamp((f - 238) / 8, 0, 1) : 0
        shotOverlay.style.opacity = String(ovOut)
        shotToolbar.style.opacity = String(ovOut)
        shotSel.style.opacity = String(ovOut)
        shotRect.style.opacity = '0'

        // 启动器弹簧入场（250+），与搜索/剪贴板等段统一
        const laIn = clamp((f - 250) / 16, 0, 1)
        const laS = spring(f, 250, 100, 16)
        const laOut = f > dis ? 1 - clamp((f - dis) / 14, 0, 1) : 1
        launcher.style.opacity = String(laIn * laOut)
        launcher.style.transform = `scale(${0.94 + laS * 0.06}) translateY(${(1 - laS) * 8}px)`
        launcher.style.filter = laIn < 1 ? `blur(${(1 - laIn) * 10}px)` : 'none'

        // 桌面窗口与启动器同步消失
        winVscode.style.opacity = String(laOut)
        winTerm.style.opacity = String(laOut * 0.96)

        extTagIcon.className = 'ext-tag-icon ri-scan-2-line'
        extTagName.textContent = 'OCR'
        extTag.style.display = 'flex'
        extTag.style.opacity = String(laIn)
        setSbText('识别结果', 'var(--color-text-muted)')
        searchCursor.style.opacity = '0'

        panelOcr.style.opacity = String(laIn)

        // 预览 + 文本整体出现（不逐字）
        ocrPreview.style.opacity = '1'
        ocrTextArea.style.opacity = String(clamp((f - 254) / 8, 0, 1))
        ocrTextArea.textContent =
          'Voidnix — macOS 效率启动器\n模块化扩展架构\nRust + Vue 3 + Tauri 2'
        ocrAction.style.opacity = String(clamp((f - 262) / 8, 0, 1))

        // 选中复制项
        const copyActive = clamp((f - 272) / 6, 0, 1)
        ocrAction.style.background =
          copyActive > 0.01 ? `rgb(${COOL_RGB} / ${copyActive * 0.11})` : 'transparent'
      }
    }
  }

  // 截屏段字幕：覆盖 renderCaption 的 seg.cap，按阶段显示不同文本
  let _shotCapPhase = -1
  function setShotCaption(f: number, phaseStart: number, text: string) {
    if (phaseStart !== _shotCapPhase) {
      _shotCapPhase = phaseStart
      _capText = ''
    }
    const fadeIn = clamp((f - phaseStart - 4) / 8, 0, 1)
    const op = fadeIn
    if (text !== _capText) {
      captionEl.textContent = text
      _capText = text
    }
    if (op !== _capOp) {
      captionEl.style.opacity = String(op)
      _capOp = op
    }
  }

  // ═══════════════════════════════════════════════
  //  段：窗口管理
  // ═══════════════════════════════════════════════
  function renderSnap(f: number) {
    winTerm.style.opacity = '0'
    const winIn = clamp((f - 8) / 14, 0, 1)
    const winOut = f > 112 ? 1 - clamp((f - 112) / 16, 0, 1) : 1
    const winOp = winIn * winOut
    if (winOp === 0 && f > 128) return

    const snapE = easeOut(clamp((f - 68) / 20, 0, 1))
    winVscode.style.opacity = String(winOp)
    winVscode.style.left = lerp(290, 12, snapE) + 'px'
    winVscode.style.top = lerp(150, 40, snapE) + 'px'
    winVscode.style.width = lerp(560, 624, snapE) + 'px'
    winVscode.style.height = lerp(360, 668, snapE) + 'px'

    if (f < 24 || f > 96) {
      cursorEl.style.opacity = '0'
    } else {
      cursorEl.style.opacity = String(
        clamp((f - 24) / 6, 0, 1) * (f > 90 ? 1 - clamp((f - 90) / 6, 0, 1) : 1),
      )
      let x: number, y: number
      if (f < 48) {
        const t = easeInOut(clamp((f - 28) / 20, 0, 1))
        x = lerp(720, 630, t)
        y = lerp(430, 8, t)
      } else if (f < 58) {
        x = 630
        y = 8
      } else if (f < 66) {
        const t = easeInOut(clamp((f - 58) / 8, 0, 1))
        x = 630
        y = lerp(8, 67, t)
      } else {
        x = 630
        y = 67
      }
      cursorEl.style.transform = `translate(${x}px, ${y}px)`
    }

    if (f < 42 || f > 92) {
      snapPanel.style.opacity = '0'
      snapTarget.classList.remove('sp-hover')
    } else {
      const spIn = easeOut(clamp((f - 42) / 8, 0, 1))
      const spOut = f > 86 ? 1 - clamp((f - 86) / 6, 0, 1) : 1
      snapPanel.style.opacity = String(spIn * spOut)
      snapPanel.style.transform = `translateX(-50%) translateY(${(1 - spIn) * -16}px)`
      if (f >= 66) snapTarget.classList.add('sp-hover')
      else snapTarget.classList.remove('sp-hover')
    }
  }

  // ═══════════════════════════════════════════════
  //  段：访达工具
  // ═══════════════════════════════════════════════
  function renderFinder(f: number) {
    const dis = _segCtx.dis
    const fwIn = clamp(f / 10, 0, 1)
    const fwOut = f > dis ? 1 - clamp((f - dis) / 12, 0, 1) : 1
    finderWin.style.opacity = String(fwIn * fwOut)
    finderWin.style.transform = `translateY(${(1 - fwIn) * 12}px)`

    renderKbd(f, '⌥', 'F', 18)

    if (f < 46 || f > dis) {
      finderPanel.style.opacity = '0'
    } else {
      const fpIn = spring(f, 46, 100, 16)
      const fpOut = f > dis - 8 ? 1 - clamp((f - dis + 8) / 8, 0, 1) : 1
      finderPanel.style.opacity = String(fpIn * fpOut)
      finderPanel.style.transform = `scale(${0.94 + spring(f, 46, 100, 16) * 0.06}) translateY(${(1 - spring(f, 46, 100, 16)) * 8}px)`
    }
  }

  // ═══════════════════════════════════════════════
  //  渲染分派
  // ═══════════════════════════════════════════════
  function renderFrame(segIdx: number, f: number) {
    if (segIdx !== _lastSeg) {
      resetStage()
      _lastSeg = segIdx
    }
    const seg = SEGMENTS[segIdx]
    _segCtx = { dur: seg.dur, dis: SEG_DIS[seg.id] ?? seg.dur - 28 }
    updateFog(f)
    switch (seg.id) {
      case 'search':
        renderSearch(f)
        break
      case 'clipboard':
        renderClipboard(f)
        break
      case 'agent':
        renderAgent(f)
        break
      case 'shot':
        renderShot(f)
        break
      case 'snap':
        renderSnap(f)
        break
      case 'finder':
        renderFinder(f)
        break
    }
    if (seg.id !== 'shot') renderCaption(segIdx, f)
  }

  return { renderFrame, resetStage }
}
