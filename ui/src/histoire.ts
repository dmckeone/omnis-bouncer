// Import Tailwind CSS for use in Histoire environment
// NOTE: Must use ./ and not @ (regular vite.config.js doesn't apply here)
import { type Vue3StorySetupApi } from '@histoire/plugin-vue'

import './histoire.css'
import './shared'

// eslint-disable-next-line @typescript-eslint/no-unused-vars
export function setupApp({ app }: Vue3StorySetupApi) {}
