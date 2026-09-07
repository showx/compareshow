import { useEffect, useMemo, useRef, useState } from 'react'
import {
  ArrowLeftRight,
  ChevronDown,
  ChevronRight,
  ChevronsDownUp,
  ChevronsUpDown,
  File,
  Folder,
  RefreshCw,
  Search,
  Trash2
} from 'lucide-react'
import type {
  CompareMode,
  CompareNode,
  CompareStats,
  FileMeta,
  FolderCompareResult,
  ItemStatus,
  LocationRef,
  SftpConnectionConfig
} from '../../../shared/types'
import { formatSize, formatTime } from '../lib/format'
import { getApi } from '../lib/api'
import { canGoUp, childLoc, joinRel, locReady, locationDisplay, parentPath } from '../lib/paths'
import PathBar from '../components/PathBar'

type ViewFilter = 'all' | 'diff' | 'left' | 'right' | 'same'

interface Props {
  left: LocationRef
  right: LocationRef
  mode: CompareMode
  ignorePatterns: string[]
  connections: SftpConnectionConfig[]
  onOpenFile: (left: LocationRef, right: LocationRef, name: string) => void
  onToast: (msg: string, error?: boolean) => void
  onNewConnection: () => void
  onLocationsChange?: (left: LocationRef, right: LocationRef) => void
  onConnectionsChange?: () => void | Promise<void>
}

interface FlatRow {
  node: CompareNode
  depth: number
}

function hasStatus(node: CompareNode, status: ItemStatus): boolean {
  if (node.status === status) return true
  return node.children?.some((c) => hasStatus(c, status)) ?? false
}

function matchesFilter(node: CompareNode, filter: ViewFilter): boolean {
  if (filter === 'all') return true
  if (filter === 'diff') return node.status !== 'same'
  if (filter === 'same') return node.status === 'same'
  if (filter === 'left') return node.status === 'leftOnly' || hasStatus(node, 'leftOnly')
  if (filter === 'right') return node.status === 'rightOnly' || hasStatus(node, 'rightOnly')
  return true
}

function collectExpanded(nodes: CompareNode[], out: Set<string>): void {
  for (const n of nodes) {
    if (n.type === 'dir' && n.status !== 'same') {
      out.add(n.relPath)
      if (n.children) collectExpanded(n.children, out)
    }
  }
}

function flatten(nodes: CompareNode[], expanded: Set<string>, depth: number, query: string, filter: ViewFilter): FlatRow[] {
  const rows: FlatRow[] = []
  for (const node of nodes) {
    if (!matchesFilter(node, filter)) continue
    if (query && !node.name.toLowerCase().includes(query) && !hasQuery(node, query)) continue
    rows.push({ node, depth })
    if (node.type === 'dir' && node.children && expanded.has(node.relPath)) {
      rows.push(...flatten(node.children, expanded, depth + 1, query, filter))
    }
  }
  return rows
}

function hasQuery(node: CompareNode, q: string): boolean {
  if (node.name.toLowerCase().includes(q)) return true
  return node.children?.some((c) => hasQuery(c, q)) ?? false
}

function allDirPaths(nodes: CompareNode[], out: Set<string>): void {
  for (const n of nodes) {
    if (n.type === 'dir') {
      out.add(n.relPath)
      if (n.children) allDirPaths(n.children, out)
    }
  }
}

export default function FolderCompareView({
  left,
  right,
  mode,
  ignorePatterns,
  connections,
  onOpenFile,
  onToast,
  onNewConnection,
  onLocationsChange,
  onConnectionsChange
}: Props) {
  const [leftLoc, setLeftLoc] = useState(left)
  const [rightLoc, setRightLoc] = useState(right)
  const [leftDraft, setLeftDraft] = useState(() => locationDisplay(left, connections) || left.path)
  const [rightDraft, setRightDraft] = useState(() => locationDisplay(right, connections) || right.path)
  const [result, setResult] = useState<FolderCompareResult | null>(null)
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [anchor, setAnchor] = useState<string | null>(null)
  const [query, setQuery] = useState('')
  const [filter, setFilter] = useState<ViewFilter>('all')
  const [busy, setBusy] = useState(false)
  const [listError, setListError] = useState<string | null>(null)
  const [progress, setProgress] = useState<{ scanned: number; currentPath: string } | null>(null)
  const [cmpMode, setCmpMode] = useState<CompareMode>(mode)
  const [deep, setDeep] = useState(false)
  const [menu, setMenu] = useState<{ x: number; y: number; path: string } | null>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const deepRef = useRef(false)

  const rows = useMemo(() => {
    if (!result) return []
    return flatten(result.tree, expanded, 0, query.trim().toLowerCase(), filter)
  }, [result, expanded, query, filter])

  const selectedNodes = useMemo(
    () => rows.filter((r) => selected.has(r.node.relPath)).map((r) => r.node),
    [rows, selected]
  )

  function commit(nextLeft: LocationRef, nextRight: LocationRef) {
    setLeftLoc(nextLeft)
    setRightLoc(nextRight)
    setLeftDraft(nextLeft.displayUrl || locationDisplay(nextLeft, connections) || nextLeft.path)
    setRightDraft(nextRight.displayUrl || locationDisplay(nextRight, connections) || nextRight.path)
    onLocationsChange?.(nextLeft, nextRight)
  }

  async function fetchList(loc: LocationRef): Promise<FileMeta[]> {
    const api = getApi()
    const res = loc.kind === 'local'
      ? await api.local.list(loc.path)
      : await api.sftp.list(loc.connectionId!, loc.path || '/')
    if (!res.ok) throw new Error(res.error)
    return res.data
  }

  function shallowStatus(left?: FileMeta, right?: FileMeta): ItemStatus {
    if (left && !right) return 'leftOnly'
    if (!left && right) return 'rightOnly'
    if (!left || !right) return 'unknown'
    if (left.type !== right.type) return 'different'
    if (left.type === 'dir') return 'unknown'
    if (left.size !== right.size) return 'different'
    if (Math.floor(left.mtime / 1000) !== Math.floor(right.mtime / 1000)) return 'different'
    return 'same'
  }

  async function listCurrent(l = leftLoc, r = rightLoc) {
    const leftOk = locReady(l)
    const rightOk = locReady(r)
    if (!leftOk && !rightOk) {
      setResult(null)
      setListError(null)
      setDeep(false)
      deepRef.current = false
      return
    }
    setBusy(true)
    setListError(null)
    setProgress(null)
    setDeep(false)
    deepRef.current = false
    try {
      const [leftList, rightList] = await Promise.all([
        leftOk ? fetchList(l) : Promise.resolve([] as FileMeta[]),
        rightOk ? fetchList(r) : Promise.resolve([] as FileMeta[])
      ])
      const names = new Set<string>()
      const leftMap = new Map<string, FileMeta>()
      const rightMap = new Map<string, FileMeta>()
      for (const e of leftList) {
        leftMap.set(e.name, e)
        names.add(e.name)
      }
      for (const e of rightList) {
        rightMap.set(e.name, e)
        names.add(e.name)
      }
      const stats: CompareStats = { same: 0, different: 0, leftOnly: 0, rightOnly: 0, files: 0, dirs: 0 }
      const tree = [...names]
        .map((name) => {
          const leftMeta = leftMap.get(name)
          const rightMeta = rightMap.get(name)
          const type = (leftMeta?.type || rightMeta?.type || 'file') as CompareNode['type']
          const status = shallowStatus(leftMeta, rightMeta)
          if (type === 'dir') stats.dirs += 1
          else {
            stats.files += 1
            if (status === 'same') stats.same += 1
            else if (status === 'different') stats.different += 1
            else if (status === 'leftOnly') stats.leftOnly += 1
            else if (status === 'rightOnly') stats.rightOnly += 1
          }
          return {
            name,
            relPath: name,
            type,
            status,
            left: leftMeta,
            right: rightMeta
          } satisfies CompareNode
        })
        .sort((a, b) => {
          if (a.type !== b.type) return a.type === 'dir' ? -1 : 1
          return a.name.localeCompare(b.name, 'zh')
        })
      setResult({ tree, stats })
      setExpanded(new Set())
      setSelected(new Set())
      setAnchor(null)
      listRef.current?.focus()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setResult(null)
      setListError(msg)
      onToast(msg, true)
    } finally {
      setBusy(false)
    }
  }

  async function runCompare(l = leftLoc, r = rightLoc, m = cmpMode) {
    const leftOk = locReady(l)
    const rightOk = locReady(r)
    if (!leftOk || !rightOk) {
      onToast('两侧都填好路径后才能比较', true)
      await listCurrent(l, r)
      return
    }
    setBusy(true)
    setListError(null)
    setProgress({ scanned: 0, currentPath: '' })
    setDeep(true)
    deepRef.current = true
    try {
      const off = getApi().compare.onProgress((p) => setProgress(p))
      const res = await getApi().compare.folder({
        left: l,
        right: r,
        mode: m,
        ignorePatterns
      })
      off()
      if (!res.ok) {
        setResult(null)
        setListError(res.error)
        onToast(res.error, true)
        return
      }
      setResult(res.data)
      const exp = new Set<string>()
      collectExpanded(res.data.tree, exp)
      setExpanded(exp)
      setSelected(new Set())
      setAnchor(null)
      listRef.current?.focus()
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setResult(null)
      setListError(msg)
      onToast(msg, true)
    } finally {
      setBusy(false)
      setProgress(null)
    }
  }

  async function refresh() {
    if (deepRef.current) await runCompare()
    else await listCurrent()
  }

  useEffect(() => {
    void listCurrent(leftLoc, rightLoc)
  }, [leftLoc.path, leftLoc.connectionId, leftLoc.kind, rightLoc.path, rightLoc.connectionId, rightLoc.kind])

  function toggle(node: CompareNode) {
    setExpanded((prev) => {
      const next = new Set(prev)
      if (next.has(node.relPath)) next.delete(node.relPath)
      else next.add(node.relPath)
      return next
    })
  }

  function selectRow(relPath: string, e: React.MouseEvent) {
    if (e.shiftKey && anchor) {
      const i1 = rows.findIndex((r) => r.node.relPath === anchor)
      const i2 = rows.findIndex((r) => r.node.relPath === relPath)
      if (i1 >= 0 && i2 >= 0) {
        const [a, b] = i1 < i2 ? [i1, i2] : [i2, i1]
        setSelected(new Set(rows.slice(a, b + 1).map((r) => r.node.relPath)))
        return
      }
    }
    if (e.ctrlKey || e.metaKey) {
      setSelected((prev) => {
        const next = new Set(prev)
        if (next.has(relPath)) next.delete(relPath)
        else next.add(relPath)
        return next
      })
      setAnchor(relPath)
      return
    }
    setSelected(new Set([relPath]))
    setAnchor(relPath)
  }

  function openNode(node: CompareNode) {
    if (node.type === 'dir') {
      enterDir(node)
      return
    }
    const l = node.left ? childLoc(leftLoc, node.name, node.left.path) : { ...leftLoc, path: '' }
    const r = node.right ? childLoc(rightLoc, node.name, node.right.path) : { ...rightLoc, path: '' }
    onOpenFile(l, r, node.name)
  }

  function enterDir(node: CompareNode) {
    if (node.type !== 'dir') return
    const nextLeft = node.left
      ? { ...leftLoc, path: node.left.path, displayUrl: undefined }
      : locReady(leftLoc)
        ? childLoc(leftLoc, node.name)
        : leftLoc
    const nextRight = node.right
      ? { ...rightLoc, path: node.right.path, displayUrl: undefined }
      : locReady(rightLoc)
        ? childLoc(rightLoc, node.name)
        : rightLoc
    commit(nextLeft, nextRight)
  }

  async function copyNodes(nodes: CompareNode[], dir: 'toRight' | 'toLeft') {
    const list = nodes.filter((n) => (dir === 'toRight' ? n.left : n.right))
    if (list.length === 0) {
      onToast('没有可复制的项目', true)
      return
    }
    for (const node of list) {
      const srcMeta = dir === 'toRight' ? node.left : node.right
      if (!srcMeta) continue
      const source: LocationRef = dir === 'toRight'
        ? { ...leftLoc, path: srcMeta.path }
        : { ...rightLoc, path: srcMeta.path }
      const parentRel = node.relPath.split('/').slice(0, -1).join('/')
      const targetDir = joinRel(dir === 'toRight' ? rightLoc : leftLoc, parentRel)
      const res = await window.api.transfer.copy(source, targetDir, node.type === 'dir')
      if (!res.ok) {
        onToast(res.error, true)
        return
      }
    }
    onToast(dir === 'toRight' ? `已复制 ${list.length} 项到右侧` : `已复制 ${list.length} 项到左侧`)
    await refresh()
  }

  function collectSync(nodes: CompareNode[], dir: 'toRight' | 'toLeft', out: CompareNode[]): void {
    for (const n of nodes) {
      const take =
        dir === 'toRight'
          ? (n.status === 'leftOnly' || n.status === 'different') && n.left
          : (n.status === 'rightOnly' || n.status === 'different') && n.right
      if (take && (n.type === 'file' || n.status === 'leftOnly' || n.status === 'rightOnly')) {
        out.push(n)
        continue
      }
      if (n.children) collectSync(n.children, dir, out)
    }
  }

  async function sync(dir: 'toRight' | 'toLeft') {
    if (!result) return
    const items: CompareNode[] = []
    collectSync(result.tree, dir, items)
    if (items.length === 0) {
      onToast('没有需要同步的差异')
      return
    }
    const label = dir === 'toRight' ? '右侧' : '左侧'
    if (!confirm(`把 ${items.length} 个差异项复制到${label}？`)) return
    await copyNodes(items, dir)
  }

  async function removeSide(side: 'left' | 'right') {
    const list = selectedNodes.filter((n) => (side === 'left' ? n.left : n.right))
    if (list.length === 0) return
    if (!confirm(`确定删除${side === 'left' ? '左侧' : '右侧'}的 ${list.length} 个项目？不可撤销。`)) return
    for (const node of list) {
      const meta = side === 'left' ? node.left : node.right
      if (!meta) continue
      const loc: LocationRef = side === 'left' ? { ...leftLoc, path: meta.path } : { ...rightLoc, path: meta.path }
      const res = await window.api.transfer.remove(loc, node.type === 'dir')
      if (!res.ok) {
        onToast(res.error, true)
        return
      }
    }
    onToast('已删除')
    await refresh()
  }

  function goUp() {
    commit(
      canGoUp(leftLoc) ? parentPath(leftLoc) : leftLoc,
      canGoUp(rightLoc) ? parentPath(rightLoc) : rightLoc
    )
  }

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement)?.tagName
      if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') {
        if (e.key === 'F5') {
          e.preventDefault()
          void refresh()
        }
        return
      }
      if (e.key === 'F5') {
        e.preventDefault()
        void refresh()
        return
      }
      if (e.key === 'Escape') setMenu(null)
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'a') {
        e.preventDefault()
        setSelected(new Set(rows.map((r) => r.node.relPath)))
        return
      }
      if (e.key === 'Backspace') {
        e.preventDefault()
        goUp()
        return
      }
      if (e.key === 'Enter' && selectedNodes[0]) {
        e.preventDefault()
        if (e.ctrlKey) openNode(selectedNodes[0])
        else if (selectedNodes[0].type === 'dir') toggle(selectedNodes[0])
        else openNode(selectedNodes[0])
        return
      }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault()
        if (rows.length === 0) return
        const cur = anchor ? rows.findIndex((r) => r.node.relPath === anchor) : -1
        const next = e.key === 'ArrowDown'
          ? Math.min(rows.length - 1, cur + 1)
          : Math.max(0, cur < 0 ? 0 : cur - 1)
        const path = rows[next].node.relPath
        setAnchor(path)
        if (e.shiftKey && cur >= 0) {
          const [a, b] = cur < next ? [cur, next] : [next, cur]
          setSelected(new Set(rows.slice(a, b + 1).map((r) => r.node.relPath)))
        } else {
          setSelected(new Set([path]))
        }
        document.querySelector(`[data-row="${CSS.escape(path)}"]`)?.scrollIntoView({ block: 'nearest' })
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  })

  useEffect(() => {
    const close = () => setMenu(null)
    window.addEventListener('click', close)
    return () => window.removeEventListener('click', close)
  }, [])

  const stats: CompareStats = result?.stats || { same: 0, different: 0, leftOnly: 0, rightOnly: 0, files: 0, dirs: 0 }
  const anyReady = locReady(leftLoc) || locReady(rightLoc)
  const menuNode = menu ? rows.find((r) => r.node.relPath === menu.path)?.node : undefined

  return (
    <div className="compare" onClick={() => setMenu(null)}>
      <div className="toolbar">
        <button className="ghost-btn" onClick={() => void listCurrent()} disabled={busy || (!locReady(leftLoc) && !locReady(rightLoc))}>
          列出当前
        </button>
        <button className="primary-btn" onClick={() => void runCompare()} disabled={busy || !locReady(leftLoc) || !locReady(rightLoc)}>
          <RefreshCw size={14} /> 比较
        </button>
        <button className="ghost-btn" disabled={!selectedNodes.some((n) => n.left)} onClick={() => void copyNodes(selectedNodes, 'toRight')}>
          复制到右 →
        </button>
        <button className="ghost-btn" disabled={!selectedNodes.some((n) => n.right)} onClick={() => void copyNodes(selectedNodes, 'toLeft')}>
          ← 复制到左
        </button>
        <button className="ghost-btn" disabled={!result || busy} onClick={() => void sync('toRight')}>同步差异到右</button>
        <button className="ghost-btn" disabled={!result || busy} onClick={() => void sync('toLeft')}>同步差异到左</button>
        <div className="sep" />
        <button className="icon-btn" title="删除左侧选中" disabled={!selectedNodes.some((n) => n.left)} onClick={() => void removeSide('left')}>
          <Trash2 size={14} />
        </button>
        <button className="icon-btn" title="删除右侧选中" disabled={!selectedNodes.some((n) => n.right)} onClick={() => void removeSide('right')}>
          <Trash2 size={14} />
        </button>
        <div className="sep" />
        <button className="icon-btn" title="展开全部" onClick={() => {
          if (!result) return
          const s = new Set<string>()
          allDirPaths(result.tree, s)
          setExpanded(s)
        }}>
          <ChevronsUpDown size={14} />
        </button>
        <button className="icon-btn" title="折叠全部" onClick={() => setExpanded(new Set())}>
          <ChevronsDownUp size={14} />
        </button>
        <select value={cmpMode} onChange={(e) => {
          const m = e.target.value as CompareMode
          setCmpMode(m)
          if (deepRef.current) void runCompare(leftLoc, rightLoc, m)
        }}>
          <option value="quick">按时间/大小</option>
          <option value="content">按文件内容</option>
        </select>
        <div className="grow" />
        <div className="search-wrap">
          <Search size={14} />
          <input className="search" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="过滤文件名" />
        </div>
      </div>

      <div className="pathbars">
        <PathBar
          side="left"
          value={leftLoc}
          draft={leftDraft}
          connections={connections}
          onDraft={setLeftDraft}
          onChange={(loc) => commit(loc, rightLoc)}
          onNewConnection={onNewConnection}
          onConnectionsChange={onConnectionsChange}
          onToast={onToast}
        />
        <div className="mid-tools">
          <button className="icon-btn" title="交换左右" onClick={() => commit(rightLoc, leftLoc)}>
            <ArrowLeftRight size={14} />
          </button>
        </div>
        <PathBar
          side="right"
          value={rightLoc}
          draft={rightDraft}
          connections={connections}
          onDraft={setRightDraft}
          onChange={(loc) => commit(leftLoc, loc)}
          onNewConnection={onNewConnection}
          onConnectionsChange={onConnectionsChange}
          onToast={onToast}
        />
      </div>

      <div className="filterbar">
        <button className={filter === 'all' ? 'on' : ''} onClick={() => setFilter('all')}>全部</button>
        <button className={filter === 'diff' ? 'on' : ''} onClick={() => setFilter('diff')}>差异 {stats.different + stats.leftOnly + stats.rightOnly}</button>
        <button className={filter === 'left' ? 'on' : ''} onClick={() => setFilter('left')}>仅左 {stats.leftOnly}</button>
        <button className={filter === 'right' ? 'on' : ''} onClick={() => setFilter('right')}>仅右 {stats.rightOnly}</button>
        <button className={filter === 'same' ? 'on' : ''} onClick={() => setFilter('same')}>相同 {stats.same}</button>
        <span className="hint-inline">列出只读当前目录 · 比较才扫描子目录 · 双击文件夹进入 · 双击文件对比</span>
      </div>

      <div className="table-head">
        <div className="pane-head"><span>名称</span><span>大小</span><span>修改时间</span></div>
        <div className="center-col">复制</div>
        <div className="pane-head"><span>名称</span><span>大小</span><span>修改时间</span></div>
      </div>

      <div className="rows" ref={listRef} tabIndex={0}>
        {canGoUp(leftLoc) || canGoUp(rightLoc) ? (
          <div className="row parent-row" onDoubleClick={goUp} onClick={() => setSelected(new Set())}>
            <div className="pane-cell left-cell">
              <div className="name-cell"><Folder size={14} /><span className="label">..</span></div>
            </div>
            <div className="center-col" />
            <div className="pane-cell right-cell">
              <div className="name-cell"><Folder size={14} /><span className="label">..</span></div>
            </div>
          </div>
        ) : null}

        {!anyReady && (
          <div className="empty-guide">
            <h3>点「列出」查看磁盘，或粘贴路径回车</h3>
            <p>
              支持资源管理器「复制路径」、
              <code> D:\code </code>
              、
              <code> %USERPROFILE%\Desktop </code>
              。远程用
              <code> sftp://root@主机 </code>
              。
            </p>
          </div>
        )}
        {listError && (
          <div className="empty list-error">{listError}</div>
        )}
        {busy && !result && anyReady && !listError && (
          <div className="empty">
            {progress ? `正在比较 ${progress.scanned} 项… ${progress.currentPath}` : '正在读取当前目录…'}
          </div>
        )}
        {rows.map(({ node, depth }) => (
          <div
            key={node.relPath}
            data-row={node.relPath}
            className={`row status-${node.status} ${selected.has(node.relPath) ? 'selected' : ''}`}
            onClick={(e) => selectRow(node.relPath, e)}
            onDoubleClick={() => openNode(node)}
            onContextMenu={(e) => {
              e.preventDefault()
              if (!selected.has(node.relPath)) setSelected(new Set([node.relPath]))
              setAnchor(node.relPath)
              setMenu({ x: e.clientX, y: e.clientY, path: node.relPath })
            }}
          >
            <PaneCell side="left" node={node} depth={depth} expanded={expanded.has(node.relPath)} onTwist={() => toggle(node)} />
            <div className="center-col row-ops">
              <button
                className="copy-arr"
                title="复制到右侧"
                disabled={!node.left || node.status === 'same'}
                onClick={(e) => { e.stopPropagation(); void copyNodes([node], 'toRight') }}
              >→</button>
              <button
                className="copy-arr"
                title="复制到左侧"
                disabled={!node.right || node.status === 'same'}
                onClick={(e) => { e.stopPropagation(); void copyNodes([node], 'toLeft') }}
              >←</button>
            </div>
            <PaneCell side="right" node={node} depth={depth} expanded={expanded.has(node.relPath)} onTwist={() => toggle(node)} />
          </div>
        ))}
        {anyReady && !busy && !listError && result && rows.length === 0 && (
          <div className="empty">这一视图没有项目。试试点「全部」，或换一个过滤条件。</div>
        )}
        {anyReady && !busy && !listError && !result && (
          <div className="empty">无法读取目录。检查路径后点「列出」或回车重试。</div>
        )}
      </div>

      {menu && menuNode && (
        <div className="ctx" style={{ left: menu.x, top: menu.y }} onClick={(e) => e.stopPropagation()}>
          {menuNode.type === 'file' && <button onClick={() => { openNode(menuNode); setMenu(null) }}>比较内容</button>}
          {menuNode.type === 'dir' && <button onClick={() => { enterDir(menuNode); setMenu(null) }}>进入此文件夹</button>}
          {menuNode.type === 'dir' && menuNode.children?.length ? (
            <button onClick={() => { toggle(menuNode); setMenu(null) }}>展开 / 折叠</button>
          ) : null}
          <button disabled={!menuNode.left} onClick={() => { void copyNodes(selectedNodes.length ? selectedNodes : [menuNode], 'toRight'); setMenu(null) }}>复制到右侧</button>
          <button disabled={!menuNode.right} onClick={() => { void copyNodes(selectedNodes.length ? selectedNodes : [menuNode], 'toLeft'); setMenu(null) }}>复制到左侧</button>
          <button disabled={!menuNode.left} onClick={() => { void removeSide('left'); setMenu(null) }}>删除左侧</button>
          <button disabled={!menuNode.right} onClick={() => { void removeSide('right'); setMenu(null) }}>删除右侧</button>
        </div>
      )}

      <div className="statusbar">
        {busy && progress ? (
          <span className="progress">扫描子目录 {progress.scanned} · {progress.currentPath}</span>
        ) : (
          <>
            <button className="dot same" onClick={() => setFilter('same')}><i />相同 {stats.same}</button>
            <button className="dot diff" onClick={() => setFilter('diff')}><i />不同 {stats.different}</button>
            <button className="dot left" onClick={() => setFilter('left')}><i />仅左 {stats.leftOnly}</button>
            <button className="dot right" onClick={() => setFilter('right')}><i />仅右 {stats.rightOnly}</button>
            <span>已选 {selected.size} 项</span>
          </>
        )}
        <div className="grow" />
        <span>{deep ? (cmpMode === 'quick' ? '已比较整棵树 · 大小与时间' : '已比较整棵树 · 文件内容 MD5') : '当前目录（未进入子文件夹）'}</span>
      </div>
    </div>
  )
}

function PaneCell({
  side,
  node,
  depth,
  expanded,
  onTwist
}: {
  side: 'left' | 'right'
  node: CompareNode
  depth: number
  expanded: boolean
  onTwist: () => void
}) {
  const meta = side === 'left' ? node.left : node.right
  const missing = !meta
  return (
    <div className={`pane-cell ${side}-cell ${missing ? 'missing' : ''}`} style={{ paddingLeft: 8 + depth * 16 }}>
      <div className="name-cell">
        {node.type === 'dir' && node.children?.length ? (
          <button className="twist" onClick={(e) => { e.stopPropagation(); onTwist() }}>
            {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          </button>
        ) : <span className="twist-spacer" />}
        {node.type === 'dir' ? <Folder size={14} /> : <File size={14} />}
        <span className="label">{meta?.name || (missing ? '' : node.name)}</span>
      </div>
      <span className="num">{meta && node.type === 'file' ? formatSize(meta.size) : ''}</span>
      <span className="num">{meta && node.type === 'file' ? formatTime(meta.mtime) : ''}</span>
    </div>
  )
}
