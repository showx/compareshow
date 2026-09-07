import { ArrowLeftRight, FolderTree, HardDrive, Plus, Server, Trash2 } from 'lucide-react'
import type { LocationKind, LocationRef, RecentSession, SftpConnectionConfig } from '../../../shared/types'
import LocationPicker from '../components/LocationPicker'

interface Props {
  connections: SftpConnectionConfig[]
  recent: RecentSession[]
  setup: SetupState | null
  onSetup: (s: SetupState | null) => void
  onChangeSetup: (s: SetupState) => void
  onStart: () => void
  onOpenRecent: (r: RecentSession) => void
  onNewConnection: () => void
  onEditConnection: (c: SftpConnectionConfig) => void
  onDeleteConnection: (id: string) => void
}

export interface SetupState {
  leftKind: LocationKind
  rightKind: LocationKind
  left: LocationRef
  right: LocationRef
  ignore: string
  mode: 'quick' | 'content'
}

export default function HomeView({
  connections,
  recent,
  setup,
  onSetup,
  onChangeSetup,
  onStart,
  onOpenRecent,
  onNewConnection,
  onEditConnection,
  onDeleteConnection
}: Props) {
  return (
    <div className="home">
      <div className="hero">
        <div>
          <h1>CompareShow</h1>
          <p>面向本地与 SFTP 的专业文件对比。并排查看目录差异、逐行比对文本、一键同步到另一侧——工作方式接近 Beyond Compare。</p>
        </div>
        <button className="primary-btn" onClick={onNewConnection}><Plus size={14} /> 新建 SFTP 连接</button>
      </div>

      <div className="mode-grid">
        <button className="mode-card" onClick={() => onSetup(makeSetup('local', 'local'))}>
          <div className="icon-row"><HardDrive size={18} /><ArrowLeftRight size={16} /></div>
          <h3>本地 ↔ 本地</h3>
          <span>比较两台磁盘、备份目录或发布前后的文件夹。</span>
        </button>
        <button className="mode-card" onClick={() => onSetup(makeSetup('local', 'sftp'))}>
          <div className="icon-row"><FolderTree size={18} /><ArrowLeftRight size={16} /></div>
          <h3>本地 ↔ SFTP</h3>
          <span>把本机项目与服务器目录对齐，适合部署核对与热修。</span>
        </button>
        <button className="mode-card" onClick={() => onSetup(makeSetup('sftp', 'sftp'))}>
          <div className="icon-row"><Server size={18} /><ArrowLeftRight size={16} /></div>
          <h3>SFTP ↔ SFTP</h3>
          <span>对比两台远程主机，例如预发与生产、主备站点。</span>
        </button>
      </div>

      {setup && (
        <div className="overlay" onMouseDown={() => onSetup(null)}>
          <div className="dialog wide" onMouseDown={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h3>新建比较会话</h3>
            </div>
            <div className="dialog-body">
              <div className="pair">
                <LocationPicker
                  title={setup.leftKind === 'local' ? '本地' : 'SFTP'}
                  side="left"
                  kind={setup.leftKind}
                  connections={connections}
                  value={setup.left}
                  onChange={(left) => onChangeSetup({ ...setup, left })}
                  onNewConnection={onNewConnection}
                />
                <button
                  className="ghost-btn swap"
                  title="交换两侧"
                  onClick={() => onChangeSetup({
                    ...setup,
                    leftKind: setup.rightKind,
                    rightKind: setup.leftKind,
                    left: setup.right,
                    right: setup.left
                  })}
                >
                  <ArrowLeftRight size={16} />
                </button>
                <LocationPicker
                  title={setup.rightKind === 'local' ? '本地' : 'SFTP'}
                  side="right"
                  kind={setup.rightKind}
                  connections={connections}
                  value={setup.right}
                  onChange={(right) => onChangeSetup({ ...setup, right })}
                  onNewConnection={onNewConnection}
                />
              </div>
              <div className="form-grid" style={{ marginTop: 14 }}>
                <div className="field">
                  <label>比较方式</label>
                  <select value={setup.mode} onChange={(e) => onChangeSetup({ ...setup, mode: e.target.value as 'quick' | 'content' })}>
                    <option value="quick">快速（大小 + 修改时间）</option>
                    <option value="content">内容（MD5，更准更慢）</option>
                  </select>
                </div>
                <div className="field">
                  <label>忽略规则（逗号或换行，支持 * 通配）</label>
                  <input
                    value={setup.ignore}
                    onChange={(e) => onChangeSetup({ ...setup, ignore: e.target.value })}
                    placeholder="node_modules,.git,*.map,.DS_Store"
                  />
                </div>
              </div>
            </div>
            <div className="dialog-foot">
              <button className="ghost-btn" onClick={() => onSetup(null)}>取消</button>
              <button className="primary-btn" onClick={onStart}>开始比较</button>
            </div>
          </div>
        </div>
      )}

      <div className="section-head">
        <h2>已保存的连接</h2>
      </div>
      {connections.length === 0 ? (
        <div className="empty">还没有 SFTP 连接。先保存一台服务器，之后比较时可以直接选用。</div>
      ) : (
        <div className="conn-grid">
          {connections.map((c) => (
            <div key={c.id} className="conn-card">
              <div className="avatar"><Server size={16} /></div>
              <div className="meta">
                <div className="name">{c.name}</div>
                <div className="sub">{c.username}@{c.host}:{c.port} {c.defaultPath}</div>
              </div>
              <button className="ghost-btn" onClick={() => onEditConnection(c)}>编辑</button>
              <button className="icon-btn" onClick={() => onDeleteConnection(c.id)}><Trash2 size={14} /></button>
            </div>
          ))}
        </div>
      )}

      <div className="section-head" style={{ marginTop: 28 }}>
        <h2>最近会话</h2>
      </div>
      {recent.length === 0 ? (
        <div className="empty">比较过的路径会出现在这里，方便下次直接打开。</div>
      ) : (
        <div className="recent-list">
          {recent.map((r) => (
            <button key={r.id} className="recent-card" onClick={() => onOpenRecent(r)}>
              <div className="avatar"><FolderTree size={16} /></div>
              <div className="meta">
                <div className="name">{r.title}</div>
                <div className="sub">{r.leftPath}  ↔  {r.rightPath}</div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

export function makeSetup(leftKind: LocationKind, rightKind: LocationKind): SetupState {
  return {
    leftKind,
    rightKind,
    left: { kind: leftKind, path: leftKind === 'local' ? '' : '/', connectionId: undefined },
    right: { kind: rightKind, path: rightKind === 'local' ? '' : '/', connectionId: undefined },
    ignore: 'node_modules,.git,.svn,dist,out,.DS_Store,Thumbs.db',
    mode: 'quick'
  }
}
