import { registerMessages } from '@/runtime/i18n'
import { zhCNMessages } from './zh-CN'
import { enMessages } from './en'

// 注册框架级文案（main.ts import 即生效）
registerMessages(zhCNMessages)
registerMessages(enMessages)
