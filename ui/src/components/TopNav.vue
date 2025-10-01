<script setup lang="ts">
import { API_URI } from '@/constants.ts'

const props = defineProps<{
  title?: string
}>()

const docsURI = API_URI + 'docs'
</script>

<template>
  <div class="min-h-screen">
    <div class="navbar bg-base-100 shadow-sm">
      <div class="flex-1">
        <a v-if="props.title != undefined" class="btn btn-ghost text-xl">{{ props.title }}</a>
      </div>
      <div class="flex-none">
        <ul class="menu menu-horizontal px-1">
          <li><a :href="docsURI">API Docs</a></li>
        </ul>
      </div>
    </div>
    <div class="flex fixed w-screen">
      <div class="flex flex-col">
        <slot></slot>
      </div>
    </div>
    <div class="w-5"></div>
  </div>
</template>

<style scoped></style>
