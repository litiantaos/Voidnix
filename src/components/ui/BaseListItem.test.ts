import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { defineComponent, h, ref, nextTick } from 'vue'
import BaseListItem from './BaseListItem.vue'

// 动态 slot 存在性翻转：`<template v-if #trailing>` 经编译为父 render 期决定 slots
// 集合项（条件不在 slot 函数内），slot 集合形态不参与响应式、由父 render 驱动同步。
// BaseListItem 的空判定因此必须在 render 期求值——computed 缓存在 slot 函数内的
// 响应式读取失效后重算为 false，缺席分支依赖集为空，false 永久锁死，slot 再度传入
// 也不恢复（brew 更新按钮「拉取更新」完成后消失且不回来的根因）。
describe('BaseListItem 动态 slot 翻转', () => {
  it('trailing slot 消失后再出现，容器与内容必须恢复渲染', async () => {
    // slot 函数内的响应式读取：模拟真实消费（brew 按钮读 running/runningStep），
    // 使 computed 依赖非空、具备失效途径——这是锁死 false 的必要条件
    const touch = ref(0)
    const show = ref(true)
    const wrapper = mount(
      defineComponent({
        setup: () => () =>
          h(
            BaseListItem,
            { title: 't' },
            show.value
              ? {
                  trailing: () => {
                    void touch.value
                    return h('button', { class: 'tb' }, 'x')
                  },
                }
              : {},
          ),
      }),
    )
    expect(wrapper.find('.tb').exists()).toBe(true)

    // 先令 slot 内依赖失效（对应 brew 运行态清空），再撤掉 slot（对应 v-if 翻 false）
    touch.value++
    show.value = false
    await nextTick()
    expect(wrapper.find('.tb').exists()).toBe(false)

    // slot 再度传入（对应新状态 has_update=true）：必须恢复
    show.value = true
    await nextTick()
    expect(wrapper.find('.tb').exists()).toBe(true)
    wrapper.unmount()
  })
})
