export interface ParsedSftpUrl {
  username?: string
  password?: string
  host: string
  port?: number
  path: string
  scheme: 'sftp' | 'ssh' | 'scp'
}

export function looksLikeSftpInput(raw: string): boolean {
  const text = raw.trim()
  if (!text) return false
  if (/^(sftp|scp|ssh):\/\//i.test(text)) return true
  if (/^[a-zA-Z]:[\\/]/.test(text) || text.startsWith('\\\\') || text.startsWith('/')) return false
  return /^[^\s@/\\]+@[^@\s/\\]+/.test(text)
}

export function parseSftpUrl(raw: string): ParsedSftpUrl | null {
  const text = raw.trim()
  if (!text) return null

  const withScheme = /^(?:sftp|scp|ssh):\/\/(.+)$/i.exec(text)
  const scheme = (/^(sftp|scp|ssh):\/\//i.exec(text)?.[1]?.toLowerCase() || 'sftp') as ParsedSftpUrl['scheme']
  const rest = withScheme ? withScheme[1] : text
  if (!withScheme && !looksLikeSftpInput(text)) return null

  let username = ''
  let password: string | undefined
  let hostportpath = rest

  const at = rest.lastIndexOf('@')
  if (at >= 0) {
    const userinfo = rest.slice(0, at)
    hostportpath = rest.slice(at + 1)
    const colon = userinfo.indexOf(':')
    if (colon >= 0) {
      username = decode(userinfo.slice(0, colon))
      password = decode(userinfo.slice(colon + 1))
    } else {
      username = decode(userinfo)
    }
  }

  let host = hostportpath
  let port: number | undefined
  let path = '/'

  const slash = hostportpath.indexOf('/')
  let hostport = hostportpath
  if (slash >= 0) {
    hostport = hostportpath.slice(0, slash)
    path = hostportpath.slice(slash) || '/'
  } else {
    const scp = /^([^:]+):([^/].*)$/.exec(hostportpath)
    if (scp && !/^\d+$/.test(scp[2])) {
      hostport = scp[1]
      path = scp[2].startsWith('/') ? scp[2] : `/${scp[2]}`
    }
  }

  const portMatch = /^(.+):(\d+)$/.exec(hostport)
  if (portMatch) {
    host = portMatch[1]
    port = Number(portMatch[2]) || 22
  } else {
    host = hostport
  }

  host = host.replace(/^\[|\]$/g, '').trim()
  if (!host) return null
  if (!path.startsWith('/')) path = `/${path}`
  if (path.length > 1 && path.endsWith('/')) path = path.replace(/\/+$/, '') || '/'

  return { username: username || undefined, password, host, port, path, scheme }
}

export function formatSftpUrl(p: {
  username?: string
  host: string
  port?: number
  path: string
  scheme?: 'sftp' | 'ssh' | 'scp'
}): string {
  const scheme = p.scheme === 'ssh' ? 'ssh' : 'sftp'
  const user = p.username ? `${p.username}@` : ''
  const port = p.port && p.port !== 22 ? `:${p.port}` : ''
  const path = p.path || '/'
  return `${scheme}://${user}${p.host}${port}${path}`
}

function decode(value: string): string {
  try {
    return decodeURIComponent(value)
  } catch {
    return value
  }
}
