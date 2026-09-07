import { useEffect, useState } from 'react'
import { X } from 'lucide-react'
import type { SftpConnectionConfig } from '../../../shared/types'

interface Props {
  initial?: Partial<SftpConnectionConfig> | null
  onClose: () => void
  onSaved: (c: SftpConnectionConfig) => void
}

const blank: SftpConnectionConfig = {
  id: '',
  name: '',
  host: '',
  port: 22,
  username: '',
  authType: 'password',
  password: '',
  privateKeyPath: '',
  passphrase: '',
  defaultPath: '/',
  createdAt: 0,
  lastUsedAt: 0
}

export default function ConnectDialog({ initial, onClose, onSaved }: Props) {
  const [form, setForm] = useState<SftpConnectionConfig>({ ...blank, ...initial })
  const [testing, setTesting] = useState(false)
  const [saving, setSaving] = useState(false)
  const [msg, setMsg] = useState<string | null>(null)
  const [err, setErr] = useState<string | null>(null)

  useEffect(() => {
    setForm({ ...blank, ...initial })
  }, [initial])

  function set<K extends keyof SftpConnectionConfig>(key: K, value: SftpConnectionConfig[K]) {
    setForm((f) => ({ ...f, [key]: value }))
  }

  async function pickKey() {
    const p = await window.api.dialog.openFile('选择私钥文件')
    if (p) set('privateKeyPath', p)
  }

  async function test() {
    setTesting(true)
    setErr(null)
    setMsg(null)
    const res = await window.api.sftp.test(form)
    setTesting(false)
    if (!res.ok) setErr(res.error)
    else setMsg('连接成功')
  }

  async function save() {
    if (!form.host || !form.username) {
      setErr('请填写主机和用户名')
      return
    }
    setSaving(true)
    setErr(null)
    const name = form.name || `${form.username}@${form.host}`
    const saved = await window.api.connections.save({ ...form, name })
    setSaving(false)
    onSaved(saved)
  }

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="dialog narrow" onMouseDown={(e) => e.stopPropagation()}>
        <div className="dialog-head">
          <h3>{form.id ? '编辑 SFTP 连接' : '新建 SFTP 连接'}</h3>
          <button className="icon-btn" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="dialog-body">
          <p className="hint">会自动使用 ~/.ssh 里的私钥和 ssh-agent。密码仅在密钥登录失败时才需要。</p>
          <div className="form-grid">
            <div className="field span2">
              <label>连接名称</label>
              <input value={form.name} onChange={(e) => set('name', e.target.value)} placeholder="例如：生产环境" />
            </div>
            <div className="field">
              <label>主机</label>
              <input value={form.host} onChange={(e) => set('host', e.target.value)} placeholder="sftp.example.com" />
            </div>
            <div className="field">
              <label>端口</label>
              <input type="number" value={form.port} onChange={(e) => set('port', Number(e.target.value) || 22)} />
            </div>
            <div className="field">
              <label>用户名</label>
              <input value={form.username} onChange={(e) => set('username', e.target.value)} />
            </div>
            <div className="field">
              <label>认证方式</label>
              <select value={form.authType} onChange={(e) => set('authType', e.target.value as 'password' | 'privateKey')}>
                <option value="password">密码</option>
                <option value="privateKey">私钥</option>
              </select>
            </div>
            {form.authType === 'password' ? (
              <div className="field span2">
                <label>密码</label>
                <input type="password" value={form.password || ''} onChange={(e) => set('password', e.target.value)} />
              </div>
            ) : (
              <>
                <div className="field span2">
                  <label>私钥路径</label>
                  <div className="path-row">
                    <input value={form.privateKeyPath || ''} onChange={(e) => set('privateKeyPath', e.target.value)} />
                    <button className="ghost-btn" onClick={pickKey}>浏览</button>
                  </div>
                </div>
                <div className="field span2">
                  <label>私钥口令（可选）</label>
                  <input type="password" value={form.passphrase || ''} onChange={(e) => set('passphrase', e.target.value)} />
                </div>
              </>
            )}
            <div className="field span2">
              <label>默认远程路径</label>
              <input value={form.defaultPath || '/'} onChange={(e) => set('defaultPath', e.target.value)} />
            </div>
          </div>
          {msg && <p className="hint" style={{ color: 'var(--ok)', marginTop: 12 }}>{msg}</p>}
          {err && <p className="hint" style={{ color: 'var(--danger)', marginTop: 12 }}>{err}</p>}
        </div>
        <div className="dialog-foot">
          <button className="ghost-btn" onClick={test} disabled={testing}>{testing ? '测试中…' : '测试连接'}</button>
          <button className="primary-btn" onClick={save} disabled={saving}>{saving ? '保存中…' : '保存'}</button>
        </div>
      </div>
    </div>
  )
}
