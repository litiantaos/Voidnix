import { computed } from 'vue'
import { useSettingsStore } from '@/stores/settings'

export function useShortcutConfig(extId: string, defaultShortcut: string) {
  const settings = useSettingsStore()
  const value = computed(() => settings.getShortcutOverride(extId) ?? defaultShortcut)
  const update = (val: string) => settings.setShortcutOverride(extId, val)
  return { value, update }
}
