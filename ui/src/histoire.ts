// Import Tailwind CSS for use in Histoire environment
// NOTE: Must use ./ and not @ (regular vite.config.js doesn't apply here)
import { type Vue3StorySetupApi } from '@histoire/plugin-vue'

import './shared'

export function setupApp({ app }: Vue3StorySetupApi) {}
