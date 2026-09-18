<template>
  <span class="action-menu-hint" aria-hidden="true">
    <span class="action-menu-hint-scrim" />
    <span class="action-menu-hint-key">
      <span>⌘</span>
      <i class="i-ri-corner-down-left-line" />
    </span>
  </span>
</template>

<style scoped>
/*
 * Cmd+Enter 动作菜单（右键菜单同入口）快捷键键帽：absolute 悬浮于所在列表行
 * （BaseList [role=option]，行 wrapper relative 作锚）右缘，容器上下撑满行高，
 * key 经 flex 垂直居中——脱流不占行内布局空间。
 * scrim：键帽底部伸出的透明模糊遮罩，backdrop blur 糊掉行尾内容（source /
 * SparkLine），mask 渐变（左透明 → 32px 处全模糊）无硬切边缘、无实色垫底，
 * 与任意行底色（含选中 ui-active 色块）天然衔接，明暗主题自动跟随。
 * ⌘ 与回车图标（corner-down-left，替代字形 ↩）flex 排列，间距由 gap 控制。
 * 仅所在行选中（ui-active，含多选）时显现，hover 不触发，显隐瞬时无过渡。
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
  /* visibility（非 opacity）：未选中行整树跳过绘制，scrim 的 backdrop blur 不参与合成 */
  visibility: hidden;
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

[role='option'].ui-active .action-menu-hint {
  visibility: visible;
}
</style>
