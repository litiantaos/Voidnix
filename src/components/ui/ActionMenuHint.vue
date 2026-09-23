<template>
  <span class="action-menu-hint" :class="{ show: visible }" aria-hidden="true">
    <span class="action-menu-hint-scrim" />
    <span class="action-menu-hint-key">
      <span>⌘</span>
      <i class="i-ri-corner-down-left-line" />
    </span>
  </span>
</template>

<script setup lang="ts">
withDefaults(defineProps<{ visible?: boolean }>(), { visible: false })
</script>

<style scoped>
/*
 * Cmd+Enter 动作菜单（右键菜单同入口）快捷键键帽：渲染于 BaseList 选中滑层
 * （.selection-hint）内，随滑层 translateY 平滑移动到焦点行——运动由滑层承载，
 * 显隐只表达「焦点行有无动作」的数据语义（visible = actionHint 谓词按焦点行求值），
 * 翻转时淡入淡出。上下撑满行高（滑层高 = 焦点行高），key 经 flex 垂直居中——
 * 脱流不占行内布局空间。
 * scrim：键帽底部伸出的透明模糊遮罩，backdrop blur 糊掉行尾内容（source /
 * SparkLine），mask 渐变（左透明 → 32px 处全模糊）无硬切边缘、无实色垫底，
 * 与任意行底色（含选中 ui-active 色块）天然衔接，明暗主题自动跟随。
 * ⌘ 与回车图标（corner-down-left，替代字形 ↩）flex 排列，间距由 gap 控制。
 * 淡入淡出挂在玻璃（scrim）与内容（key）子元素自身：祖先 opacity 形成 group
 * opacity 会隔断 scrim 的 backdrop 背景采样，毛玻璃先不显、到位后遮罩突现（同
 * mica-bar/acrylic-bar 玻璃控件淡入走自身 opacity 的纪律，见 PinWindow）；进场
 * 150ms ease-out 淡入、退场 100ms ease-in 淡出，退场完成后容器延迟交还
 * visibility——稳态下未显示时整树跳过绘制（scrim 的 backdrop blur 不参与合成）。
 * pointer-events: none 装饰性穿透，不挡行尾点击与右键。
 */
.action-menu-hint {
  position: absolute;
  top: 0;
  bottom: 0;
  right: 12px;
  display: inline-flex;
  align-items: center;
  pointer-events: none;
  visibility: hidden;
  /* 退场：淡出后延迟交还 visibility（时长与子元素淡出同档）；淡入淡出在子元素自身 */
  transition: visibility 0s linear var(--duration-fastest);
}

.action-menu-hint-scrim {
  position: absolute;
  /* 右 -12px 抵消容器 right: 12px——遮罩右缘贴齐行色块边缘；右侧圆角随行 radius-panel */
  inset: 0 -12px 0 -20px;
  border-radius: 0 10px 10px 0;
  backdrop-filter: blur(var(--soft-surface-blur));
  -webkit-backdrop-filter: blur(var(--soft-surface-blur));
  -webkit-mask-image: linear-gradient(to right, transparent 0, rgba(0, 0, 0, 0.82) 32px);
  mask-image: linear-gradient(to right, transparent 0, rgba(0, 0, 0, 0.82) 32px);
}

.action-menu-hint-key {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 3px 6px;
  border: 1px solid var(--color-divider);
  border-radius: 4px;
  background: var(--color-surface);
  font-size: 11px;
  font-weight: 500;
  line-height: 1;
  color: var(--color-text-muted);
}

/* 淡出基态（见头部注释的 group opacity 纪律）；显隐由 visible 驱动的 show 类翻转 */
.action-menu-hint-scrim,
.action-menu-hint-key {
  opacity: 0;
  transition: opacity var(--duration-fastest) var(--ease-in);
}

.action-menu-hint.show {
  visibility: visible;
  /* visibility 即时切换（覆盖基态的延迟交还），淡入在子元素 */
  transition: visibility 0s;
}

.action-menu-hint.show .action-menu-hint-scrim,
.action-menu-hint.show .action-menu-hint-key {
  opacity: 1;
  transition: opacity var(--duration-fast) var(--ease-out);
}
</style>
