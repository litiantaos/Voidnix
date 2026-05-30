import pluginVue from 'eslint-plugin-vue'
import vueTsEslintConfig from '@vue/eslint-config-typescript'
import unocss from '@unocss/eslint-plugin'
import eslintConfigPrettier from 'eslint-config-prettier'

export default [
  {
    name: 'app/files-to-lint',
    files: ['**/*.{ts,mts,tsx,vue}'],
  },

  {
    name: 'app/files-to-ignore',
    ignores: ['**/dist/**', '**/dist-ssr/**', '**/coverage/**', 'src-tauri/**'],
  },

  ...pluginVue.configs['flat/essential'],
  ...vueTsEslintConfig(),
  unocss.configs.flat,

  {
    name: 'app/modules-naming',
    files: ['extensions/**/*.vue'],
    rules: {
      'vue/multi-word-component-names': 'off',
    },
  },

  eslintConfigPrettier,
]
