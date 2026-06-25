import { ref, type Ref } from 'vue'

export function useInputControl<T extends HTMLInputElement | HTMLTextAreaElement>(options: {
  modelValue: Ref<string>
  emit: {
    (e: 'update:modelValue', value: string): void
    (e: 'keydown', event: KeyboardEvent): void
  }
}) {
  const elRef = ref<T>()

  function onInput(e: Event) {
    options.emit('update:modelValue', (e.target as T).value)
  }

  function onKeydown(e: KeyboardEvent) {
    options.emit('keydown', e)
  }

  function focus(options?: FocusOptions) {
    elRef.value?.focus(options)
  }

  function blur() {
    elRef.value?.blur()
  }

  return { elRef, onInput, onKeydown, focus, blur }
}
