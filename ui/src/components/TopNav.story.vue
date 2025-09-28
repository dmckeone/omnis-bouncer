<script setup lang="ts">
import TopNav from './TopNav.vue'
import { type Ref, ref } from 'vue'

const title: Ref<string> = ref('My Title')
</script>

<template>
  <Story auto-props-disabled responsive-disabled>
    <TopNav :title="title"></TopNav>

    <template #controls>
      <HstText v-model="title" title="Title" />
    </template>
  </Story>
</template>

<style scoped></style>
