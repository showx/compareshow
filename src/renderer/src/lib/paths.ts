import type { LocationRef, SftpConnectionConfig } from '../../../shared/types'
import { formatSftpUrl } from '../../../shared/sftp-url'

export const DEFAULT_IGNORE = ['node_modules', '.git', '.svn', 'dist', 'out', '.DS_Store', 'Thumbs.db']

export function localSep(path: string): string {
  return path.includes('\\') ? '\\' : '/'
}

export function isWinDrivesRoot(path: string): boolean {
  return path === '\\' || path === '/'
}

export function normalizeLocalInput(input: string): string {
  let text = (input || '').trim()
  if (!text) return ''
  if (
    (text.startsWith('"') && text.endsWith('"') && text.length >= 2) ||
    (text.startsWith("'") && text.endsWith("'") && text.length >= 2)
  ) {
    text = text.slice(1, -1).trim()
  }
  if (/^file:/i.test(text)) {
    text = decodeURIComponent(text.replace(/^file:\/\/\/?/i, ''))
    if (/^\/[a-zA-Z]:/.test(text)) text = text.slice(1)
  }
  if (text === '此电脑') return '\\'
  if (/^[a-zA-Z]:$/.test(text)) return `${text}\\`
  if (/^[a-zA-Z]:[\\/]/.test(text) || text.startsWith('\\\\')) return text.replace(/\//g, '\\')
  return text
}

export function looksLikeLocalPath(raw: string): boolean {
  const text = raw.trim().replace(/^['"]+|['"]+$/g, '')
  if (!text) return false
  if (/^file:/i.test(text)) return true
  if (/^[a-zA-Z]:[\\/]?/.test(text)) return true
  if (text === '\\' || text === '此电脑' || text.startsWith('\\\\')) return true
  if (text === '~' || text.startsWith('~/') || text.startsWith('~\\')) return true
  if (/^%[A-Za-z0-9_]+%/.test(text)) return true
  return false
}

export function parentPath(loc: LocationRef): LocationRef {
  if (loc.kind === 'local') {
    if (isWinDrivesRoot(loc.path)) return loc
    const sep = localSep(loc.path)
    const normalized = loc.path.replace(/[\\/]+$/, '')
    if (/^[a-zA-Z]:$/.test(normalized)) return { ...loc, path: '\\' }
    const idx = Math.max(normalized.lastIndexOf('\\'), normalized.lastIndexOf('/'))
    if (idx <= 0) return loc
    const parent = normalized.slice(0, idx)
    return { ...loc, path: /^[a-zA-Z]:$/.test(parent) ? parent + sep : parent }
  }
  const next = loc.path.replace(/\/+$/, '').split('/').slice(0, -1).join('/') || '/'
  return { ...loc, path: next, displayUrl: undefined }
}

export function childLoc(loc: LocationRef, name: string, metaPath?: string): LocationRef {
  if (metaPath) return { ...loc, path: metaPath }
  if (loc.kind === 'local') {
    const sep = localSep(loc.path)
    return { ...loc, path: loc.path.replace(/[\\/]+$/, '') + sep + name }
  }
  const base = loc.path === '/' ? '' : loc.path.replace(/\/+$/, '')
  return { ...loc, path: `${base}/${name}` }
}

export function joinRel(loc: LocationRef, rel: string): LocationRef {
  if (!rel) return loc
  if (loc.kind === 'local') {
    const sep = localSep(loc.path)
    return { ...loc, path: loc.path.replace(/[\\/]+$/, '') + sep + rel.split('/').join(sep) }
  }
  const base = loc.path === '/' ? '' : loc.path.replace(/\/+$/, '')
  return { ...loc, path: `${base}/${rel}` }
}

export function canGoUp(loc: LocationRef): boolean {
  if (!loc.path) return false
  if (loc.kind === 'sftp') return loc.path !== '/'
  if (isWinDrivesRoot(loc.path)) return false
  const n = loc.path.replace(/[\\/]+$/, '')
  if (/^[a-zA-Z]:$/.test(n)) return true
  return /[\\/]/.test(n) || /^[a-zA-Z]:\\.+/.test(loc.path)
}

export function locReady(loc: LocationRef): boolean {
  if (loc.kind === 'sftp') return Boolean(loc.connectionId)
  return Boolean(loc.path)
}

export function locationDisplay(loc: LocationRef, connections: SftpConnectionConfig[]): string {
  if (loc.kind !== 'sftp') return isWinDrivesRoot(loc.path) ? '此电脑' : loc.path
  const c = connections.find((x) => x.id === loc.connectionId)
  if (!c) return loc.path
  return formatSftpUrl({ username: c.username, host: c.host, port: c.port, path: loc.path || '/' })
}
