import { useEffect, useState } from 'react'
import { Plus, X } from 'lucide-react'
import type { CompareMode, LocationRef, RecentSession, SftpConnectionConfig } from '../../shared/types'
import ConnectDialog from './components/ConnectDialog'
import Sidebar from './components/Sidebar'
import FolderCompareView from './views/FolderCompareView'
import FileCompareView from './views/FileCompareView'
import { DEFAULT_IGNORE } from './lib/paths'

type Tab =
  | {
      id: string
      kind: 'folder'
      title: string
      left: LocationRef
      right: LocationRef
      mode: CompareMode
      ignorePatterns: string[]
    }
  | { id: string; kind: 'file'; title: string; left: LocationRef; right: LocationRef }

function emptyFolderTab(): Tab {
  return {
    id: crypto.randomUUID(),
    kind: 'folder',
    title: '文件夹比较',
    left: { kind: 'local', path: '' },
    right: { kind: 'local', path: '' },
    mode: 'quick',
    ignorePatterns: DEFAULT_IGNORE
  }
}

export default function App() {
  const [tabs, setTabs] = useState<Tab[]>(() => [emptyFolderTab()])
  const [activeId, setActiveId] = useState('')
  const [connections, setConnections] = useState<SftpConnectionConfig[]>([])
  const [recent, setRecent] = useState<RecentSession[]>([])
  const [connectOpen, setConnectOpen] = useState(false)
  const [editing, setEditing] = useState<SftpConnectionConfig | null>(null)
  const [toast, setToast] = useState<{ text: string; error?: boolean } | null>(null)

  const active = tabs.find((t) => t.id === activeId) || tabs[0]

  async function reload() {
    setConnections(await window.api.connections.list())
    setRecent(await window.api.recent.list())
  }

  useEffect(() => {
    void reload()
  }, [])

  useEffect(() => {
    if (!toast) return
    const t = setTimeout(() => setToast(null), 3200)
    return () => clearTimeout(t)
  }, [toast])

  function showToast(text: string, error = false) {
    setToast({ text, error })
  }

  function addFolderTab(left?: LocationRef, right?: LocationRef, title?: string) {
    const tab: Tab = {
      ...emptyFolderTab(),
      left: left || { kind: 'local', path: '' },
      right: right || { kind: 'local', path: '' },
      title: title || '文件夹比较'
    }
    setTabs((prev) => [...prev, tab])
    setActiveId(tab.id)
  }

  function openRecent(r: RecentSession) {
    addFolderTab(
      { kind: r.leftKind, path: r.leftPath, connectionId: r.leftConnectionId },
      { kind: r.rightKind, path: r.rightPath, connectionId: r.rightConnectionId },
      r.title
    )
  }

  function remember(left: LocationRef, right: LocationRef, title: string) {
    if (!left.path || !right.path) return
    void window.api.recent.add({
      id: crypto.randomUUID(),
      title,
      leftKind: left.kind,
      rightKind: right.kind,
      leftPath: left.path,
      rightPath: right.path,
      leftConnectionId: left.connectionId,
      rightConnectionId: right.connectionId,
      at: Date.now()
    }).then(() => reload())
  }

  if (!active) return null

  return (
    <div className="app">
      <header className="titlebar">
        <div className="brand">
          <span className="brand-mark" />
          CompareShow
        </div>
        <div className="tabs">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              className={`tab ${tab.id === active.id ? 'active' : ''}`}
              onClick={() => setActiveId(tab.id)}
            >
              <span className="tab-label">{tab.title}</span>
              {tabs.length > 1 && (
                <span
                  className="close"
                  onClick={(e) => {
                    e.stopPropagation()
                    setTabs((prev) => {
                      const next = prev.filter((t) => t.id !== tab.id)
                      if (next.length === 0) {
                        const fresh = emptyFolderTab()
                        setActiveId(fresh.id)
                        return [fresh]
                      }
                      if (active.id === tab.id) setActiveId(next[next.length - 1].id)
                      return next
                    })
                  }}
                >
                  <X size={12} />
                </span>
              )}
            </button>
          ))}
          <button className="tab add-tab" title="新建比较" onClick={() => addFolderTab()}>
            <Plus size={14} />
          </button>
        </div>
      </header>

      <div className="workspace">
        <Sidebar
          connections={connections}
          recent={recent}
          onNewCompare={() => addFolderTab()}
          onOpenRecent={openRecent}
          onNewConnection={() => { setEditing(null); setConnectOpen(true) }}
          onEditConnection={(c) => { setEditing(c); setConnectOpen(true) }}
        />
        {active.kind === 'folder' && (
          <FolderCompareView
            key={active.id}
            left={active.left}
            right={active.right}
            mode={active.mode}
            ignorePatterns={active.ignorePatterns}
            connections={connections}
            onToast={showToast}
            onNewConnection={() => { setEditing(null); setConnectOpen(true) }}
            onConnectionsChange={() => reload()}
            onLocationsChange={(left, right) => {
              const title = shortTitle(left, right)
              setTabs((prev) => prev.map((t) => t.id === active.id && t.kind === 'folder' ? { ...t, left, right, title } : t))
              remember(left, right, title)
            }}
            onOpenFile={(l, r, name) => {
              const id = crypto.randomUUID()
              setTabs((prev) => [...prev, { id, kind: 'file', title: name, left: l, right: r }])
              setActiveId(id)
            }}
          />
        )}
        {active.kind === 'file' && (
          <FileCompareView key={active.id} left={active.left} right={active.right} onToast={showToast} />
        )}
      </div>

      {connectOpen && (
        <ConnectDialog
          initial={editing}
          onClose={() => setConnectOpen(false)}
          onSaved={async () => {
            setConnectOpen(false)
            await reload()
          }}
        />
      )}
      {toast && <div className={`toast ${toast.error ? 'error' : ''}`}>{toast.text}</div>}
    </div>
  )
}

function shortTitle(left: LocationRef, right: LocationRef): string {
  const tail = (p: string) => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p || '文件夹比较'
  if (!left.path && !right.path) return '文件夹比较'
  return `${tail(left.path)} ↔ ${tail(right.path)}`
}
