<template>
    <div
        ref="rootEl"
        class="p-2 border border-black/10 rounded-lg bg-surface/95 flex gap-2 shadow-xl items-center absolute z-50 backdrop-blur-sm"
        :style="style"
        @mousedown.stop
    >
        <!-- 工具按钮 -->
        <BaseButton
            v-for="t in tools"
            :key="t.label"
            :variant="activeTool === t.id ? 'primary' : 'default'"
            :title="t.label"
            :icon="t.icon"
            @click="$emit('tool', t.id)"
        />

        <div class="mx-0.5 bg-black/10 shrink-0 h-5 w-px" />

        <!-- 颜色选择器（模糊工具时不显示） -->
        <div
            v-if="activeTool !== 'blur'"
            class="flex h-7 items-center relative"
        >
            <button
                class="border-2 rounded-full shrink-0 h-5 w-5 shadow-sm transition-transform active:scale-95"
                :style="{
                    background: color,
                    borderColor: color === '#ffffff' ? '#d1d5db' : 'white',
                }"
                title="颜色"
                @click="showColors = !showColors"
            />

            <Transition name="palette-popup">
                <div
                    v-if="showColors"
                    class="p-2 border border-black/10 rounded-lg bg-surface/95 flex gap-2 shadow-xl items-center left-1/2 absolute z-100 backdrop-blur-sm -translate-x-1/2"
                    :class="
                        popDir === 'up' ? 'bottom-full mb-4' : 'top-full mt-4'
                    "
                    @mousedown.stop
                    @click.stop
                >
                    <button
                        v-for="c in colors"
                        :key="c"
                        class="border-2 rounded-full shrink-0 h-5 w-5 shadow-sm transition-transform active:scale-95"
                        :class="{
                            'ring-2 ring-accent ring-offset-1': c === color,
                        }"
                        :style="{
                            background: c,
                            borderColor: c === '#ffffff' ? '#d1d5db' : 'white',
                        }"
                        :title="c"
                        @click="
                            $emit('color', c);
                            showColors = false;
                        "
                    />
                </div>
            </Transition>
        </div>

        <!-- 字号（文本工具） -->
        <BaseSlider
            v-if="activeTool === 'text'"
            :model-value="fontSize"
            :min="12"
            :max="64"
            title="字号"
            @update:model-value="$emit('font-size', $event)"
        />

        <!-- 线宽（矩形/直线/箭头） -->
        <BaseSlider
            v-if="
                activeTool === 'rect' ||
                activeTool === 'line' ||
                activeTool === 'arrow'
            "
            :model-value="lineWidth"
            :min="1"
            :max="12"
            title="线宽"
            @update:model-value="$emit('line-width', $event)"
        />

        <!-- 模糊模式切换（仅 blur 工具时显示） -->
        <div v-if="activeTool === 'blur'" class="flex">
            <BaseButton
                class="rounded-r-0!"
                :variant="blurMode === 'selection' ? 'primary' : 'default'"
                title="模糊整个选区"
                icon="i-ri-checkbox-blank-line"
                @click="emit('blur-mode', 'selection')"
            />
            <BaseButton
                class="rounded-l-0!"
                :variant="blurMode === 'text' ? 'primary' : 'default'"
                title="模糊选区内文本"
                icon="i-ri-t-box-line"
                @click="emit('blur-mode', 'text')"
            />
        </div>

        <!-- 模糊度（仅 blur 工具时显示） -->
        <BaseSlider
            v-if="activeTool === 'blur'"
            :model-value="blurAmount"
            :min="5"
            :max="50"
            title="模糊度"
            @update:model-value="$emit('blur-amount', $event)"
        />

        <div class="mx-0.5 bg-black/10 shrink-0 h-5 w-px" />

        <!-- 操作按钮 -->
        <BaseButton
            title="OCR 识别"
            icon="i-ri-scan-line"
            @click="$emit('ocr')"
        />
        <BaseButton
            title="钉图"
            icon="i-ri-pushpin-line"
            @click="$emit('pin')"
        />
        <BaseButton
            title="复制 (Enter)"
            icon="i-ri-file-copy-line"
            @click="$emit('copy')"
        />
        <BaseButton title="保存" icon="i-ri-save-line" @click="$emit('save')" />
        <BaseButton
            title="取消 (Esc)"
            icon="i-ri-close-line"
            @click="$emit('cancel')"
        />
    </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted, nextTick, watch } from "vue";
import BaseButton from "@/components/ui/BaseButton.vue";
import BaseSlider from "@/components/ui/BaseSlider.vue";

type Tool = "rect" | "line" | "arrow" | "text" | "blur" | null;
type BlurMode = "selection" | "text";

const props = defineProps<{
    sel: { x: number; y: number; w: number; h: number };
    activeTool: Tool;
    color: string;
    lineWidth: number;
    fontSize: number;
    blurAmount: number;
    blurMode: BlurMode;
    screenHeight: number;
    screenWidth: number;
}>();

const emit = defineEmits<{
    (e: "tool", t: Tool): void;
    (e: "color", c: string): void;
    (e: "line-width", n: number): void;
    (e: "font-size", n: number): void;
    (e: "blur-amount", n: number): void;
    (e: "blur-mode", m: BlurMode): void;
    (e: "ocr"): void;
    (e: "pin"): void;
    (e: "copy"): void;
    (e: "save"): void;
    (e: "cancel"): void;
}>();

const showColors = ref(false);
const rootEl = ref<HTMLElement>();

// 实测尺寸：palette 在工具/选区变化后可能宽度有变，挂载/变化时同步更新。
const paletteW = ref(380);
const paletteH = ref(44);

const tools: { id: Tool; label: string; icon: string }[] = [
    { id: "rect", label: "矩形", icon: "i-ri-checkbox-blank-line" },
    { id: "line", label: "直线", icon: "i-ri-subtract-line" },
    { id: "arrow", label: "箭头", icon: "i-ri-arrow-right-line" },
    { id: "text", label: "文字", icon: "i-ri-text" },
    { id: "blur", label: "模糊", icon: "i-ri-drop-line" },
];

const colors = [
    "#ff3b30",
    "#ff9500",
    "#ffcc00",
    "#34c759",
    "#007aff",
    "#5856d6",
    "#af52de",
    "#ffffff",
    "#000000",
];

const PALETTE_GAP = 8;
const EDGE_PAD = 8;

function measure() {
    if (rootEl.value) {
        const rect = rootEl.value.getBoundingClientRect();
        if (rect.width > 0) paletteW.value = rect.width;
        if (rect.height > 0) paletteH.value = rect.height;
    }
}

onMounted(() => {
    nextTick(measure);
});

// 工具或选区切换可能改变 palette 宽度，下一帧重新测量
watch(
    () =>
        [
            props.activeTool,
            props.sel.x,
            props.sel.y,
            props.sel.w,
            props.sel.h,
        ] as const,
    () => nextTick(measure),
);

const style = computed(() => {
    const { x, y, w, h } = props.sel;
    const pw = paletteW.value;
    const ph = paletteH.value;

    // 水平：优先与选区左对齐；超出右边界则向左偏移；选区本身贴右时退回到 sel.right - pw。
    let left: number;
    if (x + pw <= props.screenWidth - EDGE_PAD) {
        left = Math.max(EDGE_PAD, x);
    } else {
        left = Math.max(
            EDGE_PAD,
            Math.min(x + w - pw, props.screenWidth - pw - EDGE_PAD),
        );
    }

    // 垂直：优先下方；不够则上方；都不够则贴在选区内底部。
    const belowTop = y + h + PALETTE_GAP;
    if (belowTop + ph <= props.screenHeight - EDGE_PAD) {
        return { top: `${belowTop}px`, left: `${left}px` };
    }
    if (y - ph - PALETTE_GAP >= EDGE_PAD) {
        return { top: `${y - ph - PALETTE_GAP}px`, left: `${left}px` };
    }
    const insideBottom = props.screenHeight - (y + h) + PALETTE_GAP;
    return {
        bottom: `${Math.max(insideBottom, EDGE_PAD)}px`,
        left: `${left}px`,
    };
});

// 颜色弹窗方向：跟随 palette 的展开方向（palette 在选区上方时弹窗也向上）。
// 这样弹窗始终背离选区，最大化利用 palette 已选好的那一侧空间。
const popDir = computed<"up" | "down">(() =>
    "bottom" in style.value ? "up" : "down",
);

// 工具切换时关闭颜色弹窗，避免 blur 工具下隐藏 + 残留状态
watch(
    () => props.activeTool,
    () => {
        showColors.value = false;
    },
);
</script>

<style scoped>
.palette-popup-enter-active,
.palette-popup-leave-active {
    transition:
        opacity 0.2s,
        transform 0.2s;
}
.palette-popup-enter-from,
.palette-popup-leave-to {
    opacity: 0;
    transform: translateY(-4px);
}
</style>
