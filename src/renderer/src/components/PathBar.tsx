import { forwardRef, useImperativeHandle, useState } from 'react'
import { ChevronUp, FolderOpen, Plug } from 'lucide-react'
import type { LocationRef, SftpConnectionConfig } from '../../../shared/types'
import { looksLikeSftpInput, parseSftpUrl } from '../../../shared/sftp-url'
import { getApi } from '../lib/api'
import { looksLikeLocalPath, normalizeLocalInput, parentPath } from '../lib/paths'
import RemoteBrowser from './RemoteBrowser'

interface Props {
  side: 'left' | 'right'
  value: LocationRef
  draft: string
  connections: SftpConnectionConfig[]
  onDraft: (path: string) => void
  onChange: (loc: LocationRef) => void
  onNewConnection: () => void
  onConnectionsChange?: () => void | Promise<void>
  onToast: (msg: string, error?: boolean) => void
}

export interface PathBarHandle {
  submit: () => Promise<void>
}

interface AuthAsk {
  url: string
  username: string
  host: string
  port: number
  message: string
}

const PathBar = forwardRef<PathBarHandle, Props>(function PathBar({
  side,
  value,
  draft,
  connections,
  onDraft,
  onChange,
  onNewConnection,
  onConnectionsChange,
  onToast
}, ref) {
  const [browse, setBrowse] = useState(false)
  const [auth, setAuth] = useState<AuthAsk | null>(null)
  const [password, setPassword] = useState('')
  const [remember, setRemember] = useState(true)
  const [connecting, setConnecting] = useState(false)
  const [status, setStatus] = useState<string | null>(null)

  useImperativeHandle(ref, () => ({ submit }))

  async function pickLocal() {
    const p = await getApi().dialog.openDirectory(side === 'left' ? '选择左侧文件夹' : '选择右侧文件夹')
    if (p) applyLocal(p)
  }

  function onSource(raw: string) {
    if (raw === '__new__') {
      onNewConnection()
      return
    }
    if (raw === 'local') {
      onChange({ kind: 'local', path: value.kind === 'local' ? value.path : '', connectionId: undefined })
      return
    }
    const c = connections.find((x) => x.id === raw)
    onChange({
      kind: 'sftp',
      connectionId: raw,
      path: c?.defaultPath || '/'
    })
  }

  async function openRemote(url: string, pass?: string) {
    setConnecting(true)
    setStatus('正在用本机 SSH 密钥连接…')
    const res = await getApi().sftp.openUrl(url, pass, remember)
    setConnecting(false)
    if (res.ok) {
      setAuth(null)
      setPassword('')
      setStatus('已连接')
      await onConnectionsChange?.()
      onDraft(res.data.displayUrl)
      onChange({ kind: 'sftp', connectionId: res.data.connectionId, path: res.data.path, displayUrl: res.data.displayUrl })
      return true
    }
    if (res.needPassword) {
      const parsed = parseSftpUrl(url)
      setStatus(null)
      setAuth({
        url,
        username: res.username || parsed?.username || 'root',
        host: res.host || parsed?.host || '',
        port: res.port || parsed?.port || 22,
        message: res.error
      })
      return false
    }
    setStatus(res.error)
    onToast(res.error, true)
    return false
  }

  function applyLocal(text: string) {
    const path = normalizeLocalInput(text) || '\\'
    setStatus(null)
    setAuth(null)
    onDraft(path === '\\' ? '此电脑' : path)
    onChange({ kind: 'local', path, connectionId: undefined })
    void refineLocal(text || path)
  }

  async function refineLocal(text: string) {
    const resolveApi = getApi().local.resolve
    if (!resolveApi) return
    try {
      const res = await Promise.race([
        resolveApi(text),
        new Promise<null>((ok) => setTimeout(() => ok(null), 2500))
      ])
      if (!res || !res.ok) return
      const path = res.data.path
      onDraft(res.data.type === 'drives' ? '此电脑' : path)
      onChange({ kind: 'local', path, connectionId: undefined })
      if (res.data.type === 'file') onToast('已打开文件所在目录')
    } catch {
      /* 列表已经开始读，不阻塞路径栏 */
    }
  }

  async function submit() {
    const text = draft.trim()
    if (!text) {
      if (value.kind === 'sftp' && value.connectionId) {
        onChange({ ...value, path: '/' })
        return
      }
      applyLocal(text || '\\')
      return
    }
    if (looksLikeLocalPath(text)) {
      applyLocal(text)
      return
    }
    if (looksLikeSftpInput(text) || /^(sftp|ssh|scp):\/\//i.test(text)) {
      const parsed = parseSftpUrl(text)
      if (!parsed) {
        onToast('无法解析地址，请用 sftp://root@主机', true)
        return
      }
      await openRemote(text, parsed.password)
      return
    }
    if (value.kind === 'sftp' && value.connectionId) {
      const path = text.startsWith('/') ? text : `/${text}`
      onChange({ ...value, path: path.replace(/\/{2,}/g, '/') || '/' })
      return
    }
    applyLocal(text)
  }

  const source = value.kind === 'local' ? 'local' : (value.connectionId || '')
  const remoteDraft = looksLikeSftpInput(draft) || /^(sftp|ssh|scp):\/\//i.test(draft.trim())

  return (
    <div className={`pathbar-wrap ${side}`}>
      <div className={`pathbar ${side}`}>
        <select className="source" value={source} onChange={(e) => onSource(e.target.value)} title="位置来源">
          <option value="local">本地</option>
          {connections.map((c) => (
            <option key={c.id} value={c.id}>{c.name}</option>
          ))}
          <option value="__new__">＋ 新建 SFTP…</option>
        </select>
        <input
          value={draft}
          onChange={(e) => { onDraft(e.target.value); setStatus(null) }}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
              e.preventDefault()
              void submit()
            }
          }}
          placeholder={remoteDraft || value.kind === 'sftp'
            ? 'sftp://root@主机 或 user@host'
            : '本地路径回车列出，例如 D:\\code'}
          spellCheck={false}
        />
        <button className="icon-btn" title="上级目录" onClick={() => onChange(parentPath(value))}>
          <ChevronUp size={15} />
        </button>
        {remoteDraft || (value.kind === 'sftp' && !looksLikeLocalPath(draft)) ? (
          <button className="primary-btn" disabled={connecting} onClick={() => void submit()}>
            <Plug size={14} /> {connecting ? '连接中' : '连接'}
          </button>
        ) : (
          <>
            <button className="primary-btn" onClick={() => void submit()}>
              列出
            </button>
            <button className="ghost-btn" onClick={() => void pickLocal()}>
              <FolderOpen size={14} /> 浏览
            </button>
          </>
        )}
        {value.kind === 'sftp' && value.connectionId && !remoteDraft && (
          <button className="ghost-btn" onClick={() => setBrowse(true)}>浏览</button>
        )}
      </div>
      {status && (
        <div className={`path-status ${!connecting && status !== '已连接' ? 'err' : ''}`}>{status}</div>
      )}
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
      {auth && (
        <div className="overlay" onMouseDown={() => !connecting && setAuth(null)}>
          <div className="dialog narrow" onMouseDown={(e) => e.stopPropagation()}>
            <div className="dialog-head">
              <h3>连接 {auth.username}@{auth.host}</h3>
            </div>
            <div className="dialog-body">
              <p className="hint">{auth.message || '密钥登录未成功。密钥来自本机 %USERPROFILE%\\.ssh，不用在软件里单独设置。'}</p>
              <div className="field">
                <label>服务器密码</label>
                <input
                  type="password"
                  autoFocus
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && password) void openRemote(auth.url, password)
                  }}
                />
              </div>
              <label className="check">
                <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
                记住密码
              </label>
            </div>
            <div className="dialog-foot">
              <button className="ghost-btn" disabled={connecting} onClick={() => setAuth(null)}>取消</button>
              <button className="primary-btn" disabled={connecting || !password} onClick={() => void openRemote(auth.url, password)}>
                {connecting ? '连接中…' : '登录'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
})

export default PathBar
