import { defineConfig } from '@/runtime/storage'

/// screenshot 扩展自管配置（持久化至 extensions/screenshot/config.json）。
///
/// 录屏参数的实际消费方是 Rust（overlay 子窗口无 store 权限，不能自己读持久化
/// 配置）；main 窗口在扩展 setup 里把字段同步给 Rust，见 index.ts。
export const config = defineConfig('extensions/screenshot/config', {
  savePath: '',
  recordFps: 30,
  recordSystemAudio: true,
  recordMicrophone: false,
  openClipAfterRecord: true,
})
