import { useEffect, useState } from 'react'
import { ChevronUp, Folder, File, X } from 'lucide-react'
import type { FileMeta } from '../../../shared/types'

interface Props {
  connectionId: string
  startPath?: string
  onClose: () => void
  onPick: (path: string) => void
}

export default function RemoteBrowser({ connectionId, startPath = '/', onClose, onPick }: Props) {
  const [path, setPath] = useState(startPath || '/')
  const [items, setItems] = useState<FileMeta[]>([])
  const [selected, setSelected] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(false)

  async function load(p: string) {
    setLoading(true)
    setError(null)
    const res = await window.api.sftp.list(connectionId, p)
    setLoading(false)
    if (!res.ok) {
      setError(res.error)
      return
    }
    setPath(p)
    setItems(res.data)
    setSelected(null)
  }

  useEffect(() => {
    void (async () => {
      const connected = await window.api.sftp.connected(connectionId)
      if (!connected) {
        const r = await window.api.sftp.connect(connectionId)
        if (!r.ok) {
          setError(r.error)
          return
        }
      }
      await load(startPath || '/')
    })()
  }, [connectionId, startPath])

  function up() {
    if (!path || path === '/') return
    const next = path.replace(/\/+$/, '').split('/').slice(0, -1).join('/') || '/'
    void load(next)
  }

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="dialog" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          <h3>浏览远程目录</h3>
          <button className="icon-btn" onClick={onClose}><X size={16} /></button>
        </div>
        <div className="dialog-body">
          <div className="browser">
            <div className="browser-path">
              <button className="icon-btn" onClick={up} title="上级目录"><ChevronUp size={16} /></button>
              <input value={path} onChange={(e) => setPath(e.target.value)} onKeyDown={(e) => e.key === 'Enter' && load(path)} />
              <button className="ghost-btn" onClick={() => load(path)}>转到</button>
            </div>
            <div className="browser-list">
              {loading && <div className="empty">正在读取…</div>}
              {error && <div className="empty" style={{ color: 'var(--danger)' }}>{error}</div>}
              {!loading && !error && items.map((it) => (
                <button
                  key={it.path}
                  className={`browser-item ${selected === it.path ? 'active' : ''}`}
                  onClick={() => setSelected(it.path)}
                  onDoubleClick={() => it.type === 'dir' && load(it.path)}
                >
                  {it.type === 'dir' ? <Folder size={14} /> : <File size={14} />}
                  <span>{it.name}</span>
                </button>
              ))}
            </div>
          </div>
        </div>
        <div className="dialog-foot">
          <button className="ghost-btn" onClick={onClose}>取消</button>
          <button className="primary-btn" onClick={() => onPick(selected && items.find((i) => i.path === selected)?.type === 'dir' ? selected : path)}>
            选择此目录
          </button>
        </div>
      </div>
    </div>
  )
}
