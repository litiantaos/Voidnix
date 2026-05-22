<template>
    <BaseList
        :items="allItems"
        v-model:selected-index="selectedIndex"
        keyboard-navigation
        :group-field="(item: ZshAsItem) => item.group"
        :group-title="(g: string) => g"
        @execute="
            (item: ZshAsItem) => {
                if (item.type === 'toggle') toggle();
            }
        "
    >
        <template #item="{ selected, setRef, select }">
            <BaseListItem
                :ref="setRef"
                title="启用终端自动建议"
                subtitle="Tab 切换备选，→ 接受，Ctrl+X 关闭"
                :selected="selected"
                @click="select()"
            >
                <template #trailing>
                    <BaseButton
                        :variant="
                            settings.zshAutosuggestionsEnabled
                                ? 'primary'
                                : 'default'
                        "
                        @click.stop="toggle"
                    >
                        {{
                            settings.zshAutosuggestionsEnabled
                                ? "已开启"
                                : "已关闭"
                        }}
                    </BaseButton>
                </template>
            </BaseListItem>
        </template>
    </BaseList>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useSettingsStore } from "@/stores/settings";
import BaseList from "@/components/ui/BaseList.vue";
import BaseListItem from "@/components/ui/BaseListItem.vue";
import BaseButton from "@/components/ui/BaseButton.vue";

interface ToggleItem {
    type: "toggle";
    group: string;
}

type ZshAsItem = ToggleItem;

const settings = useSettingsStore();
const selectedIndex = ref(0);

const toggle = async () => {
    const newVal = !settings.zshAutosuggestionsEnabled;
    await settings.setZshAutosuggestionsEnabled(newVal);
};

const allItems: ZshAsItem[] = [{ type: "toggle", group: "通用" }];
</script>
