import { execFile } from 'child_process'
import { createReadStream, createWriteStream, existsSync, type Dirent } from 'fs'
import { lstat, mkdir, readdir, readFile, rm, stat, writeFile } from 'fs/promises'
import { basename, dirname, isAbsolute, join, normalize, resolve as resolvePath, sep } from 'path'
import { homedir } from 'os'
import { fileURLToPath } from 'url'
import type { FileMeta } from '../shared/types'

function toPosix(p: string): string {
  return p.replace(/\\/g, '/')
}

export function isDrivesRoot(path: string): boolean {
  if (process.platform !== 'win32') return false
  const t = path.trim()
  return t === '\\' || t === '/' || /^[\\/]{2}\?\\$/.test(t)
}

export function friendlyFsError(err: unknown, path: string): Error {
  const code = (err as NodeJS.ErrnoException | undefined)?.code
  if (code === 'ENOENT') return new Error(`找不到路径：${path}`)
  if (code === 'ENOTDIR') return new Error(`不是文件夹：${path}`)
  if (code === 'EACCES' || code === 'EPERM') return new Error(`没有权限访问：${path}`)
  if (code === 'EINVAL') return new Error(`路径无效：${path}`)
  if (code === 'ENAMETOOLONG') return new Error(`路径过长：${path}`)
  if (code === 'EBUSY') return new Error(`路径正被占用：${path}`)
  if (code === 'ETIMEDOUT') return new Error(err instanceof Error ? err.message : `读取超时：${path}`)
  return new Error(err instanceof Error ? err.message : String(err))
}

export function normalizeLocalPath(input: string): string {
  let text = (input || '').trim()
  if (!text) return ''

  if (
    (text.startsWith('"') && text.endsWith('"') && text.length >= 2) ||
    (text.startsWith("'") && text.endsWith("'") && text.length >= 2)
  ) {
    text = text.slice(1, -1).trim()
  }

  if (/^file:/i.test(text)) {
    try {
      text = fileURLToPath(text)
    } catch {
      text = text.replace(/^file:\/\//i, '').replace(/^\/([a-zA-Z]:)/, '$1')
    }
  }

  text = text.replace(/%([^%]+)%/g, (all, name: string) => {
    const value = process.env[name] ?? process.env[name.toUpperCase()] ?? process.env[name.toLowerCase()]
    return value == null || value === '' ? all : value
  })

  if (text === '此电脑') return process.platform === 'win32' ? '\\' : homedir()
  if (text === '~') text = homedir()
  else if (text.startsWith('~/') || text.startsWith('~\\')) text = join(homedir(), text.slice(2))

  if (process.platform === 'win32') {
    if (text === '/' || text === '\\') return '\\'
    if (text.startsWith('\\\\') || text.startsWith('//')) return normalize(text.replace(/\//g, '\\'))
    text = text.replace(/\//g, '\\')
    if (/^[a-zA-Z]:$/.test(text)) text += '\\'
  }

  if (!isAbsolute(text) && process.platform === 'win32' && !/^[a-zA-Z]:[\\/]/.test(text)) {
    return resolvePath(text)
  }
  if (!isAbsolute(text) && process.platform !== 'win32') {
    return resolvePath(text)
  }
  return normalize(text)
}

function withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      const err = new Error(label)
      ;(err as NodeJS.ErrnoException).code = 'ETIMEDOUT'
      reject(err)
    }, ms)
    promise.then(
      (value) => {
        clearTimeout(timer)
        resolve(value)
      },
      (err) => {
        clearTimeout(timer)
        reject(err)
      }
    )
  })
}

export async function localResolve(input: string): Promise<{ path: string; type: 'dir' | 'file' | 'drives' }> {
  const path = normalizeLocalPath(input)
  if (!path) {
    if (process.platform === 'win32') return { path: '\\', type: 'drives' }
    throw new Error('请输入本地路径')
  }
  if (isDrivesRoot(path)) return { path: '\\', type: 'drives' }
  try {
    const s = await withTimeout(stat(path), 3000, `读取路径超时：${path}`)
    if (s.isDirectory()) return { path, type: 'dir' }
    return { path: dirname(path), type: 'file' }
  } catch (err) {
    throw friendlyFsError(err, path)
  }
}

function driveMeta(path: string): FileMeta {
  return { name: path.slice(0, 2), path, type: 'dir', size: 0, mtime: 0 }
}

async function listWindowsDrives(): Promise<FileMeta[]> {
  const names = new Set<string>()
  try {
    const stdout = await new Promise<string>((resolve, reject) => {
      execFile(
        'wmic',
        ['logicaldisk', 'get', 'name'],
        { timeout: 2500, windowsHide: true, encoding: 'utf8' },
        (err, out) => (err ? reject(err) : resolve(out))
      )
    })
    for (const line of stdout.split(/\r?\n/)) {
      const match = /^\s*([A-Za-z]:)\s*$/.exec(line)
      if (match) names.add(`${match[1].toUpperCase()}\\`)
    }
  } catch {
    /* wmic 在部分系统不可用，不要再用 existsSync 去扫盘，会卡死光驱/网络盘 */
  }
  if (names.size === 0) {
    names.add('C:\\')
    const cwd = /^([A-Za-z]:)/.exec(process.cwd())
    if (cwd) names.add(`${cwd[1].toUpperCase()}\\`)
  }
  return [...names].sort().map(driveMeta)
}

async function mapPool<T, R>(items: T[], limit: number, fn: (item: T, index: number) => Promise<R>): Promise<R[]> {
  const out: R[] = new Array(items.length)
  let next = 0
  async function worker(): Promise<void> {
    while (next < items.length) {
      const i = next++
      out[i] = await fn(items[i], i)
    }
  }
  const n = Math.min(Math.max(1, limit), Math.max(1, items.length))
  await Promise.all(Array.from({ length: n }, () => worker()))
  return out
}

async function localListInner(dir: string): Promise<FileMeta[]> {
  const resolved = normalizeLocalPath(dir)
  if (isDrivesRoot(resolved || dir)) return listWindowsDrives()
  if (!resolved) throw new Error('请输入本地路径')

  try {
    const s = await withTimeout(stat(resolved), 4000, `读取路径超时：${resolved}`)
    if (!s.isDirectory()) throw new Error(`不是文件夹：${resolved}`)
  } catch (err) {
    if (err instanceof Error && err.message.startsWith('不是文件夹')) throw err
    throw friendlyFsError(err, resolved)
  }

  let entries: Dirent[]
  try {
    entries = await withTimeout(readdir(resolved, { withFileTypes: true }), 8000, `读取目录超时：${resolved}`)
  } catch (err) {
    throw friendlyFsError(err, resolved)
  }

  const mapped = await mapPool(entries, 16, async (entry): Promise<FileMeta | null> => {
    if (entry.name === '.' || entry.name === '..') return null
    const full = join(resolved, entry.name)
    try {
      if (entry.isDirectory()) {
        return {
          name: entry.name,
          path: full,
          type: 'dir',
          size: 0,
          mtime: 0
        }
      }

      if (entry.isFile() || entry.isSymbolicLink()) {
        try {
          const s = await withTimeout(stat(full), 400, `stat timeout`)
          return {
            name: entry.name,
            path: full,
            type: s.isDirectory() ? 'dir' : 'file',
            size: s.isDirectory() ? 0 : s.size,
            mtime: s.mtimeMs,
            mode: s.mode
          }
        } catch {
          return {
            name: entry.name,
            path: full,
            type: entry.isDirectory() ? 'dir' : 'file',
            size: 0,
            mtime: 0
          }
        }
      }

      try {
        const s = await withTimeout(lstat(full), 400, `lstat timeout`)
        return {
          name: entry.name,
          path: full,
          type: s.isDirectory() ? 'dir' : 'file',
          size: s.isDirectory() ? 0 : s.size,
          mtime: s.mtimeMs,
          mode: s.mode
        }
      } catch {
        return { name: entry.name, path: full, type: 'file', size: 0, mtime: 0 }
      }
    } catch {
      return null
    }
  })

  return mapped.filter((x): x is FileMeta => Boolean(x)).sort((a, b) => {
    if (a.type !== b.type) return a.type === 'dir' ? -1 : 1
    return a.name.localeCompare(b.name, 'zh')
  })
}

export async function localList(dir: string): Promise<FileMeta[]> {
  try {
    return await withTimeout(localListInner(dir), 12000, `读取目录超时：${dir}`)
  } catch (err) {
    throw friendlyFsError(err, dir)
  }
}

export async function localStat(path: string): Promise<FileMeta> {
  const resolved = normalizeLocalPath(path)
  const s = await stat(resolved)
  return {
    name: basename(resolved),
    path: resolved,
    type: s.isDirectory() ? 'dir' : 'file',
    size: s.isDirectory() ? 0 : s.size,
    mtime: s.mtimeMs,
    mode: s.mode
  }
}

export async function localRead(path: string, maxBytes?: number): Promise<Buffer> {
  const resolved = normalizeLocalPath(path)
  if (maxBytes) {
    const s = await stat(resolved)
    if (s.size > maxBytes) throw new Error('FILE_TOO_LARGE')
  }
  return readFile(resolved)
}

export async function localWrite(path: string, data: Buffer): Promise<void> {
  const resolved = normalizeLocalPath(path)
  await mkdir(dirname(resolved), { recursive: true })
  await writeFile(resolved, data)
}

export async function localMkdir(path: string): Promise<void> {
  await mkdir(normalizeLocalPath(path), { recursive: true })
}

export async function localRemove(path: string): Promise<void> {
  await rm(normalizeLocalPath(path), { recursive: true, force: true })
}

export function localExists(path: string): boolean {
  const resolved = normalizeLocalPath(path)
  if (isDrivesRoot(resolved)) return true
  return existsSync(resolved)
}

export async function localCopyFile(src: string, dest: string): Promise<void> {
  const from = normalizeLocalPath(src)
  const to = normalizeLocalPath(dest)
  await mkdir(dirname(to), { recursive: true })
  await new Promise<void>((resolve, reject) => {
    const rs = createReadStream(from)
    const ws = createWriteStream(to)
    rs.on('error', reject)
    ws.on('error', reject)
    ws.on('close', () => resolve())
    rs.pipe(ws)
  })
}

export function joinLocal(dir: string, name: string): string {
  if (isDrivesRoot(dir)) {
    if (/^[a-zA-Z]:\\?$/.test(name)) return `${name.replace(/\\+$/, '')}\\`
    return normalizeLocalPath(name)
  }
  return join(dir, name)
}

export function parentLocal(path: string): string {
  const resolved = normalizeLocalPath(path)
  if (isDrivesRoot(resolved)) return '\\'
  const trimmed = resolved.replace(/[\\/]+$/, '')
  if (process.platform === 'win32' && /^[a-zA-Z]:$/.test(trimmed)) return '\\'
  return dirname(resolved)
}

export function posixJoin(dir: string, name: string): string {
  if (!dir || dir === '/') return `/${name}`
  return `${toPosix(dir).replace(/\/+$/, '')}/${name}`
}

export { sep as localPathSep }
