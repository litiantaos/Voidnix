import { invoke } from '@tauri-apps/api/core'
import { CMD } from '@/commands'
import { useAppStore, toastAndHide } from '@/stores/app'
import { t } from '@/runtime/i18n'
import type { FinderAction } from './shortcuts'

/** Rust 已自行 hide 的动作（勿再抢 hide / toast 时序） */
const HIDE_IN_RUST = new Set<FinderAction>(['toggle_hidden', 'new_file'])

/** 执行 finder_run_action：成功非 Rust 自 hide 动作 toast + 关窗，失败 error toast。
 *  View（操作目录）与 OpenWithView（二级 App 列表）共用。
 *  @returns 是否成功（供新建文件等决定是否关弹窗） */
export async function runAction(
  action: FinderAction,
  name?: string,
  appPath?: string,
): Promise<boolean> {
  const appStore = useAppStore()
  try {
    const msg = await invoke<string>(CMD.finderRunAction, {
      action,
      name: name ?? null,
      appPath: appPath ?? null,
    })
    if (HIDE_IN_RUST.has(action)) {
      return true
    }
    toastAndHide(msg || undefined)
    return true
  } catch (e) {
    appStore.showStatus(String(e ?? t('common.operationFailed')), { duration: 4000, kind: 'error' })
    return false
  }
}
