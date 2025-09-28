import config from './histoire.config'

const path_prefix = process.env.HISTOIRE_PATH_PREFIX

// Override to build with proper prefix for CI builds
config.vite = config.vite ?? {}
if (path_prefix != null) {
  config.vite.base = path_prefix
}

// Use hash based router mode
// https://github.com/histoire-dev/histoire/blob/main/packages/histoire-app/src/app/router.ts#L7
config.routerMode = 'hash'

export default config
