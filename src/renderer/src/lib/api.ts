export function getApi(): Window['api'] {
  const api = window.api
  if (!api) {
    throw new Error('应用接口未就绪。请关掉窗口后重新执行 npm run dev')
  }
  return api
}
