<script setup lang="ts">
const slots = defineSlots<{
  figure?: () => any
}>()
const props = withDefaults(
    defineProps<{
      title?: string
      value?: boolean | number
    }>(),
    {
      title: "Stat",
      value: 0
    }
)
</script>

<template>
  <div class="stat place-items-center">
    <div class="stat-figure" v-bind="$attrs" v-if="slots.figure">
      <slot name="figure" v-bind="$attrs"/>
    </div>
    <div class="stat-title text-bold">{{ props.title }}</div>
    <div class="stat-value" v-bind="$attrs">{{ props.value }}</div>
  </div>
</template>

<style scoped>

</style>