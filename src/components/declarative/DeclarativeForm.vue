<template>
  <div p="4" space="y-4">
    <template v-for="f in view.fields" :key="f.id">
      <label class="form-field">
        <span text="xs tx-subtle">{{ f.label }}</span>
        <BaseInput
          v-if="f.type === 'text' || f.type === 'password'"
          :model-value="formData[f.id] as string"
          :placeholder="f.placeholder"
          :type="f.type"
          @update:model-value="setField(f.id, $event)"
        />
        <BaseTextarea
          v-else-if="f.type === 'textarea'"
          :model-value="formData[f.id] as string"
          @update:model-value="setField(f.id, $event)"
        />
        <BaseSelect
          v-else-if="f.type === 'select'"
          :model-value="formData[f.id] as string"
          :options="f.options.map((o) => ({ value: o.value, label: o.label }))"
          @update:model-value="setField(f.id, $event)"
        />
        <button
          v-else-if="f.type === 'switch'"
          text="sm"
          class="ui-ctrl"
          p="x-3 y-1"
          rounded="md"
          self="start"
          :class="formData[f.id] ? 'bg-accent text-white' : 'bg-tx-faint/10 text-tx-subtle'"
          @click="setField(f.id, !formData[f.id])"
        >
          {{ formData[f.id] ? '开' : '关' }}
        </button>
        <ShortcutInput
          v-else-if="f.type === 'shortcut'"
          :model-value="formData[f.id] as string"
          @update:model-value="setField(f.id, $event)"
        />
      </label>
    </template>
    <div class="action-footer">
      <button
        v-if="view.cancel"
        text="sm tx-subtle"
        class="ui-ctrl"
        p="x-4 y-1.5"
        rounded="md"
        @click="emit('action', view.cancel.id, { form: { ...formData } })"
      >
        {{ view.cancel.title }}
      </button>
      <button
        text="sm white"
        class="ui-ctrl"
        p="x-4 y-1.5"
        rounded="md"
        bg="accent"
        @click="emit('action', view.submit.id, { form: { ...formData } })"
      >
        {{ view.submit.title }}
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, onMounted } from 'vue'
import type { FormView } from '@/types/declarative'
import BaseInput from '@/components/ui/BaseInput.vue'
import BaseTextarea from '@/components/ui/BaseTextarea.vue'
import BaseSelect from '@/components/ui/BaseSelect.vue'
import ShortcutInput from '@/components/ui/ShortcutInput.vue'

const props = defineProps<{ view: FormView }>()

const emit = defineEmits<{
  action: [actionId: string, payload: { form: Record<string, unknown> }]
}>()

const formData = reactive<Record<string, unknown>>({})

function setField(id: string, value: unknown) {
  formData[id] = value
}

onMounted(() => {
  for (const f of props.view.fields) {
    if ('default' in f && f.default !== undefined) {
      formData[f.id] = f.default
    }
  }
})
</script>
