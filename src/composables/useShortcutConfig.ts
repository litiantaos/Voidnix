import { computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'

export function useShortcutConfig(moduleId: string, defaultShortcut: string) {
  const settings = useSettingsStore()
  const value = computed(() => settings.getShortcutOverride(moduleId) ?? defaultShortcut)
  const update = (val: string) => settings.setShortcutOverride(moduleId, val)
  return { value, update }
}
