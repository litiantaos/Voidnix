import { ref } from 'vue'

/** 编辑 / 预览视图模式：模块级单例，Actions（切换）与 View（消费）共用。
 *
 *  不落 config——每次启动回到编辑态；KeepAlive 往返与窗口隐藏保持当前模式。
 *  index.ts 会 import View/Actions，状态独立成模块避免经 index 的循环依赖。 */
export const previewMode = ref(false)
