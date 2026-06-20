import pluginVue from 'eslint-plugin-vue'
import vueTsEslintConfig from '@vue/eslint-config-typescript'
import unocss from '@unocss/eslint-plugin'
import eslintConfigPrettier from 'eslint-config-prettier'

export default [
  {
    name: 'app/files-to-lint',
    files: ['**/*.{js,ts,mts,tsx,vue}'],
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

  {
    name: 'app/unused-vars',
    rules: {
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
    },
  },

  // P3-6：收紧静态规则（无需 type info，零侵入）
  {
    name: 'app/strict-baseline',
    rules: {
      // 拒绝 throw 字面量（必须 throw Error 子类，保留 stack）
      'no-throw-literal': 'error',
      // 拒绝 require()（项目是 ESM）
      '@typescript-eslint/no-require-imports': 'error',
      // 拒绝多余的非空断言（x!!）
      '@typescript-eslint/no-extra-non-null-assertion': 'error',
      // 拒绝无意义的空表达式（a && b()）
      '@typescript-eslint/no-unused-expressions': [
        'error',
        { allowShortCircuit: true, allowTernary: true },
      ],
      // console 仅警告（catch 块内的错误日志允许，但新增需自觉）
      'no-console': ['warn', { allow: ['warn', 'error'] }],
    },
  },

  // CI 脚本是 Node CLI 工具，console.log 是合法的标准输出方式
  {
    name: 'app/scripts-cli-output',
    files: ['scripts/**/*.ts'],
    rules: {
      'no-console': 'off',
    },
  },

  eslintConfigPrettier,
]
