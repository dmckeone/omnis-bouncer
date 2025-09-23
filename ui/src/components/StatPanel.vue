<script setup lang="ts">
import type {ComputedRef} from "vue";

const slots = defineSlots<{
  figure?: () => string,
  actions?: () => string
}>()
const props = withDefaults(
    defineProps<{
      title?: string
      value?: boolean | number | string | null | ComputedRef<boolean | number | string | null>
    }>(),
    {
      title: "Stat"
    }
)
</script>

<template>
  <div class="stat place-items-center">
    <div class="stat-figure" v-bind="$attrs" v-if="slots.figure">
      <slot name="figure" v-bind="$attrs"/>
    </div>
    <div class="stat-title text-bold">{{ props.title }}</div>
    <div class="stat-value" v-bind="$attrs">{{ props.value != null ? props.value : "--" }}</div>
    <div class="stat-actions">
      <slot name="actions"/>
    </div>
  </div>
</template>

<style scoped>

</style>