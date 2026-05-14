import { defineStore } from 'pinia'
import { ref } from 'vue'
import { getAllModules } from '@/core/module-registry'
import type { AppModule } from '@/types/module'

export const useModulesStore = defineStore('modules', () => {
  const modules = ref<AppModule[]>([])

  function loadModules() {
    modules.value = getAllModules()
  }

  return { modules, loadModules }
})
