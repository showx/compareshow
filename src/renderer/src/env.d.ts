import type { CompareShowApi } from '../../preload/index'

declare global {
  interface Window {
    api: CompareShowApi
  }
}

export {}
