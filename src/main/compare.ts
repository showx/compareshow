import { createHash } from 'crypto'
import { createReadStream } from 'fs'
import type {
  CompareMode,
  CompareNode,
  CompareStats,
  FileMeta,
  FolderCompareRequest,
  FolderCompareResult,
  ItemStatus,
  LocationRef
} from '../shared/types'
import * as local from './local-fs'
import * as sftp from './sftp'

export type ProgressFn = (scanned: number, currentPath: string) => void

function matchIgnore(name: string, patterns: string[]): boolean {
  return patterns.some((p) => {
    const t = p.trim()
    if (!t) return false
    if (t.includes('*')) {
      const re = new RegExp('^' + t.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*') + '$', 'i')
      return re.test(name)
    }
    return name === t
  })
}

async function listSide(loc: LocationRef): Promise<FileMeta[]> {
  if (loc.kind === 'local') return local.localList(loc.path)
  if (!loc.connectionId) throw new Error('缺少 SFTP 连接')
  return sftp.sftpList(loc.connectionId, loc.path)
}

async function childLocation(loc: LocationRef, name: string): Promise<LocationRef> {
  if (loc.kind === 'local') {
    return { ...loc, path: local.joinLocal(loc.path, name) }
  }
  return { ...loc, path: local.posixJoin(loc.path, name) }
}

async function hashLocal(path: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const hash = createHash('md5')
    const stream = createReadStream(path)
    stream.on('data', (d) => hash.update(d))
    stream.on('error', reject)
    stream.on('end', () => resolve(hash.digest('hex')))
  })
}

async function hashSftp(connectionId: string, path: string): Promise<string> {
  const buf = await sftp.sftpRead(connectionId, path)
  return createHash('md5').update(buf).digest('hex')
}

async function fileStatus(left?: FileMeta, right?: FileMeta, mode: CompareMode = 'quick', leftLoc?: LocationRef, rightLoc?: LocationRef): Promise<ItemStatus> {
  if (left && !right) return 'leftOnly'
  if (!left && right) return 'rightOnly'
  if (!left || !right) return 'unknown'
  if (left.type !== right.type) return 'different'
  if (left.type === 'dir') return 'same'
  if (left.size !== right.size) return 'different'
  if (mode === 'quick') {
    const lt = Math.floor(left.mtime / 1000)
    const rt = Math.floor(right.mtime / 1000)
    if (lt !== rt && left.size === right.size) return 'different'
    return 'same'
  }
  if (!leftLoc || !rightLoc) return left.size === right.size ? 'same' : 'different'
  try {
    const [lh, rh] = await Promise.all([
      leftLoc.kind === 'local' ? hashLocal(left.path) : hashSftp(leftLoc.connectionId!, left.path),
      rightLoc.kind === 'local' ? hashLocal(right.path) : hashSftp(rightLoc.connectionId!, right.path)
    ])
    return lh === rh ? 'same' : 'different'
  } catch {
    return 'different'
  }
}

function emptyStats(): CompareStats {
  return { same: 0, different: 0, leftOnly: 0, rightOnly: 0, files: 0, dirs: 0 }
}

function countNode(node: CompareNode, stats: CompareStats): void {
  if (node.type === 'dir') stats.dirs += 1
  else {
    stats.files += 1
    if (node.status === 'same') stats.same += 1
    else if (node.status === 'different') stats.different += 1
    else if (node.status === 'leftOnly') stats.leftOnly += 1
    else if (node.status === 'rightOnly') stats.rightOnly += 1
  }
}

function rollupStatus(node: CompareNode): ItemStatus {
  if (node.type === 'file' || !node.children?.length) return node.status
  const set = new Set(node.children.map((c) => (c.type === 'dir' ? rollupStatus(c) : c.status)))
  if (set.has('different') || (set.has('leftOnly') && set.has('rightOnly'))) {
    node.status = 'different'
    return 'different'
  }
  if (set.size === 1) {
    node.status = [...set][0]
    return node.status
  }
  if (set.has('leftOnly')) node.status = 'leftOnly'
  else if (set.has('rightOnly')) node.status = 'rightOnly'
  else if (set.has('same') && set.size === 1) node.status = 'same'
  else node.status = 'different'
  return node.status
}

export async function compareFolders(
  req: FolderCompareRequest,
  onProgress?: ProgressFn
): Promise<FolderCompareResult> {
  const stats = emptyStats()
  let scanned = 0

  async function walk(left: LocationRef, right: LocationRef, rel: string): Promise<CompareNode[]> {
    let leftList: FileMeta[] = []
    let rightList: FileMeta[] = []
    const [leftRes, rightRes] = await Promise.allSettled([listSide(left), listSide(right)])
    if (leftRes.status === 'fulfilled') leftList = leftRes.value
    if (rightRes.status === 'fulfilled') rightList = rightRes.value
    if (rel === '') {
      if (leftRes.status === 'rejected' && rightRes.status === 'rejected') {
        throw leftRes.reason instanceof Error ? leftRes.reason : new Error(String(leftRes.reason))
      }
      if (leftRes.status === 'rejected') {
        throw leftRes.reason instanceof Error ? leftRes.reason : new Error(String(leftRes.reason))
      }
      if (rightRes.status === 'rejected') {
        throw rightRes.reason instanceof Error ? rightRes.reason : new Error(String(rightRes.reason))
      }
    }

    const names = new Set<string>()
    const leftMap = new Map<string, FileMeta>()
    const rightMap = new Map<string, FileMeta>()
    for (const e of leftList) {
      if (matchIgnore(e.name, req.ignorePatterns)) continue
      leftMap.set(e.name, e)
      names.add(e.name)
    }
    for (const e of rightList) {
      if (matchIgnore(e.name, req.ignorePatterns)) continue
      rightMap.set(e.name, e)
      names.add(e.name)
    }

    const nodes: CompareNode[] = []
    const sorted = [...names].sort((a, b) => {
      const la = leftMap.get(a) || rightMap.get(a)!
      const lb = leftMap.get(b) || rightMap.get(b)!
      if (la.type !== lb.type) return la.type === 'dir' ? -1 : 1
      return a.localeCompare(b, 'zh')
    })

    for (const name of sorted) {
      const l = leftMap.get(name)
      const r = rightMap.get(name)
      const type: 'file' | 'dir' = (l?.type || r?.type || 'file') as 'file' | 'dir'
      const relPath = rel ? `${rel}/${name}` : name
      scanned += 1
      onProgress?.(scanned, relPath)

      const node: CompareNode = {
        name,
        relPath,
        type,
        status: 'unknown',
        left: l,
        right: r
      }

      if (type === 'dir') {
        if (l && r) {
          node.children = await walk(await childLocation(left, name), await childLocation(right, name), relPath)
        } else if (l) {
          node.children = await walkOnly(await childLocation(left, name), relPath, 'leftOnly')
        } else if (r) {
          node.children = await walkOnly(await childLocation(right, name), relPath, 'rightOnly')
        }
        node.status = rollupStatus(node)
      } else {
        node.status = await fileStatus(l, r, req.mode, left, right)
      }
      countNode(node, stats)
      nodes.push(node)
    }
    return nodes
  }

  async function walkOnly(loc: LocationRef, rel: string, status: ItemStatus): Promise<CompareNode[]> {
    let list: FileMeta[] = []
    try {
      list = await listSide(loc)
    } catch {
      list = []
    }
    const nodes: CompareNode[] = []
    for (const entry of list) {
      if (matchIgnore(entry.name, req.ignorePatterns)) continue
      const relPath = rel ? `${rel}/${entry.name}` : entry.name
      scanned += 1
      onProgress?.(scanned, relPath)
      const node: CompareNode = {
        name: entry.name,
        relPath,
        type: entry.type,
        status,
        left: status === 'leftOnly' ? entry : undefined,
        right: status === 'rightOnly' ? entry : undefined
      }
      if (entry.type === 'dir') {
        node.children = await walkOnly(await childLocation(loc, entry.name), relPath, status)
      }
      countNode(node, stats)
      nodes.push(node)
    }
    return nodes
  }

  const tree = await walk(req.left, req.right, '')
  return { tree, stats }
}

export async function readForCompare(loc: LocationRef, maxBytes: number): Promise<{ meta?: FileMeta; buf?: Buffer; tooLarge: boolean }> {
  try {
    const meta = loc.kind === 'local'
      ? await local.localStat(loc.path)
      : await sftp.sftpStat(loc.connectionId!, loc.path)
    if (meta.type === 'dir') return { meta, tooLarge: false }
    if (meta.size > maxBytes) return { meta, tooLarge: true }
    const buf = loc.kind === 'local'
      ? await local.localRead(loc.path, maxBytes)
      : await sftp.sftpRead(loc.connectionId!, loc.path, maxBytes)
    return { meta, buf, tooLarge: false }
  } catch (err) {
    if ((err as Error).message === 'FILE_TOO_LARGE') return { tooLarge: true }
    return { tooLarge: false }
  }
}
