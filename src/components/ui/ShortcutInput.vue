<template>
  <div
    ref="rootRef"
    data-settings-control
    class="ui-ctrl flex gap-1.5 w-36 items-center justify-center"
    tabindex="0"
    @click="startRecording"
    @blur="onBlur"
    @keydown="onKeyDown"
  >
    <template v-if="isRecording && !readyToRecord">
      <span class="text-tx-muted">...</span>
    </template>
    <template v-else-if="isRecording && readyToRecord">
      <span class="text-tx-muted animate-pulse">请按下快捷键</span>
    </template>
    <template v-else-if="keys.length">
      <kbd
        v-for="(k, i) in keys"
        :key="i"
        class="text-xs font-medium font-mono rounded bg-black/5 flex h-5 items-center justify-center"
        :class="k === 'Space' ? 'text-[8px] px-2' : 'w-5'"
      >
        {{ k }}
      </kbd>
    </template>
    <template v-else>
      <span class="text-tx-muted">未设置</span>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useTauriListener } from '@/composables/useTauriListener'
import { useAppStore } from '@/stores/app'

const appStore = useAppStore()

const props = withDefaults(
  defineProps<{
    modelValue: string
    shortcutId?: string
    maxKeys?: number
  }>(),
  {
    maxKeys: 4,
  },
)

const emit = defineEmits<{
  'update:modelValue': [string]
}>()

const rootRef = ref<HTMLElement | null>(null)
const isRecording = ref(false)
const readyToRecord = ref(false)

useTauriListener<{ shortcut: string }>('shortcut-recording-captured', (payload) => {
  if (!isRecording.value) return
  emit('update:modelValue', payload.shortcut)
  stopRecording()
  blur()
})

function focus() {
  rootRef.value?.focus()
}

function blur() {
  rootRef.value?.blur()
}

defineExpose({ focus, blur, startRecording })

const keys = computed(() => {
  if (!props.modelValue) return []
  return props.modelValue.split('+').map((k) => {
    switch (k) {
      case 'CommandOrControl':
      case 'Command':
      case 'Cmd':
        return '⌘'
      case 'Control':
      case 'Ctrl':
        return '⌃'
      case 'Alt':
      case 'Option':
        return '⌥'
      case 'Shift':
        return '⇧'
      case 'Space':
        return 'Space'
      default:
        return k.toUpperCase()
    }
  })
})

async function startRecording() {
  isRecording.value = true
  appStore.setShortcutRecording(true)

  if (props.shortcutId) {
    try {
      await invoke('start_shortcut_recording', {
        id: props.shortcutId,
      })
    } catch (e) {
      console.error('Failed to start shortcut recording:', e)
    }
  }

  readyToRecord.value = true
}

async function stopRecording() {
  isRecording.value = false
  readyToRecord.value = false
  appStore.setShortcutRecording(false)

  if (props.shortcutId) {
    invoke('stop_shortcut_recording', {
      id: props.shortcutId,
    }).catch((e) => {
      console.error('Failed to stop shortcut recording:', e)
    })
  }
}

function onBlur() {
  if (isRecording.value) {
    stopRecording()
  }
}

function onKeyDown(e: KeyboardEvent) {
  if (!isRecording.value) return
  if (!readyToRecord.value) {
    e.preventDefault()
    return
  }

  e.preventDefault()
  e.stopPropagation()

  if (e.key === 'Escape') {
    stopRecording()
    blur()
    return
  }

  if (e.key === 'Enter') {
    stopRecording()
    blur()
    return
  }

  const modifiers: string[] = []
  if (e.metaKey) modifiers.push('CommandOrControl')
  if (e.ctrlKey) modifiers.push('Control')
  if (e.altKey) modifiers.push('Alt')
  if (e.shiftKey) modifiers.push('Shift')

  const isModifier = ['Meta', 'Control', 'Alt', 'Shift'].includes(e.key)

  if (!isModifier) {
    let key = e.key
    if (e.code === 'Space') {
      key = 'Space'
    } else if (/^[a-zA-Z]$/.test(key)) {
      key = key.toUpperCase()
    }

    let finalModifiers = modifiers
    if (props.maxKeys > 0) {
      const maxModifiers = Math.max(0, props.maxKeys - 1)
      if (finalModifiers.length > maxModifiers) {
        finalModifiers = finalModifiers.slice(-maxModifiers)
      }
    }

    const shortcut = [...finalModifiers, key].join('+')
    emit('update:modelValue', shortcut)
    stopRecording()
    blur()
  }
}
</script>
