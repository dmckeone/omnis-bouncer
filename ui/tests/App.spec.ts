import { describe, it } from 'vitest'

describe('Test', () => {
  it('execute correctly', () => {})
})

// TODO: Future tests once new vitest comes out: https://github.com/vitest-dev/vitest/issues/8374
// TODO: Node 24 doesn't like fetch-mock
// import { createTestingPinia } from '@pinia/testing'
// import { mount } from '@vue/test-utils'
// import { describe, expect, it, vi } from 'vitest'
//
// import App from '../src/App.vue'
//
// describe('App', () => {
//   it('mounts renders properly', () => {
//     const wrapper = mount(App, {
//       global: {
//         plugins: [createTestingPinia({ createSpy: vi.fn })],
//       },
//     })
//     expect(wrapper.text()).toBeTruthy()
//   })
// })
