import { useState } from 'react'
import { FolderOpen } from 'lucide-react'
import type { LocationKind, LocationRef, SftpConnectionConfig } from '../../../shared/types'
import RemoteBrowser from './RemoteBrowser'

interface Props {
  title: string
  side: 'left' | 'right'
  kind: LocationKind
  connections: SftpConnectionConfig[]
  value: LocationRef
  onChange: (loc: LocationRef) => void
  onNewConnection: () => void
}

export default function LocationPicker({ title, side, kind, connections, value, onChange, onNewConnection }: Props) {
  const [browse, setBrowse] = useState(false)

  async function pickLocal() {
    const p = await window.api.dialog.openDirectory(title)
    if (p) onChange({ ...value, kind: 'local', path: p, connectionId: undefined })
  }

  function onConn(id: string) {
    const c = connections.find((x) => x.id === id)
    onChange({
      kind: 'sftp',
      connectionId: id,
      path: c?.defaultPath || '/'
    })
  }

  return (
    <div className="loc-box">
      <h4>{title}{side === 'left' ? ' · 左侧' : ' · 右侧'}</h4>
      {kind === 'sftp' && (
        <div className="field" style={{ marginBottom: 10 }}>
          <label>SFTP 连接</label>
          <div className="path-row">
            <select value={value.connectionId || ''} onChange={(e) => onConn(e.target.value)}>
              <option value="">选择已保存的连接…</option>
              {connections.map((c) => (
                <option key={c.id} value={c.id}>{c.name} ({c.username}@{c.host})</option>
              ))}
            </select>
            <button className="ghost-btn" onClick={onNewConnection}>新建</button>
          </div>
        </div>
      )}
      <div className="field">
        <label>路径</label>
        <div className="path-row">
          <input
            value={value.path}
            onChange={(e) => onChange({ ...value, path: e.target.value })}
            placeholder={kind === 'local' ? 'D:\\project' : '/var/www'}
          />
          {kind === 'local' ? (
            <button className="ghost-btn" onClick={pickLocal}><FolderOpen size={14} /> 浏览</button>
          ) : (
            <button className="ghost-btn" disabled={!value.connectionId} onClick={() => setBrowse(true)}>浏览</button>
          )}
        </div>
      </div>
      {browse && value.connectionId && (
        <RemoteBrowser
          connectionId={value.connectionId}
          startPath={value.path || '/'}
          onClose={() => setBrowse(false)}
          onPick={(path) => {
            onChange({ ...value, path })
            setBrowse(false)
          }}
        />
      )}
    </div>
  )
}
