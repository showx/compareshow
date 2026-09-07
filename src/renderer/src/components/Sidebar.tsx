import { FolderPlus, Plus, Server, Clock } from 'lucide-react'
import type { RecentSession, SftpConnectionConfig } from '../../../shared/types'

interface Props {
  connections: SftpConnectionConfig[]
  recent: RecentSession[]
  onNewCompare: () => void
  onOpenRecent: (r: RecentSession) => void
  onNewConnection: () => void
  onEditConnection: (c: SftpConnectionConfig) => void
}

export default function Sidebar({
  connections,
  recent,
  onNewCompare,
  onOpenRecent,
  onNewConnection,
  onEditConnection
}: Props) {
  return (
    <aside className="sidebar">
      <button className="primary-btn new-session" onClick={onNewCompare}>
        <FolderPlus size={14} /> 新建比较
      </button>

      <div className="side-label">最近会话</div>
      <div className="side-list">
        {recent.length === 0 && <div className="side-empty">还没有比较记录</div>}
        {recent.map((r) => (
          <button key={r.id} className="side-item" onClick={() => onOpenRecent(r)} title={`${r.leftPath}\n${r.rightPath}`}>
            <Clock size={13} />
            <span>
              <b>{r.title}</b>
              <i>{r.leftKind === 'local' ? '本地' : 'SFTP'} ↔ {r.rightKind === 'local' ? '本地' : 'SFTP'}</i>
            </span>
          </button>
        ))}
      </div>

      <div className="side-label">
        SFTP 连接
        <button className="linkish" onClick={onNewConnection}><Plus size={12} /> 新建</button>
      </div>
      <div className="side-list">
        {connections.length === 0 && <div className="side-empty">保存一台服务器后，路径栏里可以直接选</div>}
        {connections.map((c) => (
          <button key={c.id} className="side-item" onClick={() => onEditConnection(c)} title={`${c.username}@${c.host}`}>
            <Server size={13} />
            <span>
              <b>{c.name}</b>
              <i>{c.username}@{c.host}</i>
            </span>
          </button>
        ))}
      </div>
    </aside>
  )
}
