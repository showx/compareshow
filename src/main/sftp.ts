import { Client, type ConnectConfig, type SFTPWrapper } from 'ssh2'
import { existsSync, readFileSync } from 'fs'
import type { FileMeta, SftpConnectionConfig } from '../shared/types'
import { agentPath, isAgentFailure, isAuthFailure, isHostUnreachable, resolveSshTarget, sshDir, SshConnectError } from './ssh-auth'

const POOL = new Map<string, LiveConnection>()

interface LiveConnection {
  id: string
  client: Client
  sftp: SFTPWrapper
  config: SftpConnectionConfig
}

function isDirMode(mode: number): boolean {
  return (mode & 0o170000) === 0o040000
}

function toMeta(dir: string, filename: string, attrs: { size: number; mtime: number; mode: number }): FileMeta {
  const type = isDirMode(attrs.mode) ? 'dir' : 'file'
  const path = dir === '/' ? `/${filename}` : `${dir.replace(/\/+$/, '')}/${filename}`
  return {
    name: filename,
    path,
    type,
    size: type === 'dir' ? 0 : Number(attrs.size) || 0,
    mtime: (Number(attrs.mtime) || 0) * 1000,
    mode: attrs.mode
  }
}

export async function connectSftp(config: SftpConnectionConfig): Promise<void> {
  await disconnectSftp(config.id)
  const resolved = resolveSshTarget(config.host, config.username || undefined, config.port || undefined)
  const username = config.username || resolved.username
  const host = resolved.host
  const port = config.port || resolved.port || 22
  const passphrase = config.passphrase || undefined
  const password = config.password || undefined

  const keyFiles = [
    config.privateKeyPath,
    ...resolved.identityFiles
  ].filter((p): p is string => Boolean(p) && existsSync(p))

  const agent = agentPath()
  const attempts: Array<{ label: string; opts: ConnectConfig }> = []
  const base: ConnectConfig = {
    host,
    port,
    username,
    readyTimeout: 12000,
    keepaliveInterval: 15000,
    tryKeyboard: true
  }

  for (const file of keyFiles) {
    try {
      attempts.push({
        label: file,
        opts: { ...base, privateKey: readFileSync(file), passphrase }
      })
    } catch {
      /* skip */
    }
  }
  if (agent) attempts.push({ label: 'ssh-agent', opts: { ...base, agent } })
  if (password) {
    attempts.push({
      label: 'password',
      opts: { ...base, password }
    })
  }

  if (attempts.length === 0) {
    throw new SshConnectError(
      `本机没有可用的 SSH 密钥。已查找 ${sshDir()}\\id_ed25519 和 id_rsa。这台服务器需要密码，或把私钥放到该目录。`,
      'AUTH',
      []
    )
  }

  let client: Client | null = null
  let hostChecked = false
  for (const attempt of attempts) {
    try {
      client = await openClient(attempt.opts)
      break
    } catch (err) {
      if (isHostUnreachable(err)) {
        throw new SshConnectError(
          `连不上 ${host}:${port}。请确认服务器 SSH 已开、安全组放行 22 端口、本机网络可达。`,
          'HOST',
          keyFiles
        )
      }
      if (isAgentFailure(err)) continue
      if (isAuthFailure(err)) {
        hostChecked = true
        continue
      }
      if (!hostChecked) throw err
    }
  }
  if (!client) {
    throw new SshConnectError(
      keyFiles.length
        ? `本机密钥未被服务器接受（已试：${keyFiles.map((f) => f.split(/[\\/]/).pop()).join('、')}）。请输入密码。`
        : 'SSH 认证失败，请输入密码。',
      'AUTH',
      keyFiles
    )
  }

  const sftpClient = await new Promise<SFTPWrapper>((resolve, reject) => {
    client!.sftp((err, sftpWrapper) => {
      if (err) reject(err)
      else resolve(sftpWrapper)
    })
  })

  POOL.set(config.id, { id: config.id, client, sftp: sftpClient, config: { ...config, host, port, username } })
}

function openClient(opts: ConnectConfig): Promise<Client> {
  const client = new Client()
  return new Promise((resolve, reject) => {
    const password = typeof opts.password === 'string' ? opts.password : ''
    const onReady = () => {
      cleanup()
      resolve(client)
    }
    const onError = (err: Error) => {
      cleanup()
      try {
        client.end()
      } catch {
        /* ignore */
      }
      reject(err)
    }
    const onKeyboard = (
      _name: string,
      _instructions: string,
      _lang: string,
      prompts: Array<{ prompt: string }>,
      finish: (responses: string[]) => void
    ) => {
      finish(prompts.map(() => password || String(opts.passphrase || '')))
    }
    const cleanup = () => {
      client.removeListener('ready', onReady)
      client.removeListener('error', onError)
      client.removeListener('keyboard-interactive', onKeyboard)
    }
    client.once('ready', onReady)
    client.once('error', onError)
    client.on('keyboard-interactive', onKeyboard)
    client.connect(opts)
  })
}

export async function disconnectSftp(id: string): Promise<void> {
  const live = POOL.get(id)
  if (!live) return
  POOL.delete(id)
  try {
    live.client.end()
  } catch {
    /* ignore */
  }
}

export function hasConnection(id: string): boolean {
  return POOL.has(id)
}

function requireLive(id: string): LiveConnection {
  const live = POOL.get(id)
  if (!live) throw new Error('SFTP 未连接，请先测试并连接服务器')
  return live
}

export async function sftpList(id: string, dir: string): Promise<FileMeta[]> {
  const { sftp } = requireLive(id)
  const target = dir || '/'
  const list = await new Promise<Array<{ filename: string; attrs: { size: number; mtime: number; mode: number } }>>(
    (resolve, reject) => {
      sftp.readdir(target, (err, entries) => {
        if (err) reject(err)
        else resolve(entries)
      })
    }
  )
  return list
    .filter((e) => e.filename !== '.' && e.filename !== '..')
    .map((e) => toMeta(target, e.filename, e.attrs))
    .sort((a, b) => {
      if (a.type !== b.type) return a.type === 'dir' ? -1 : 1
      return a.name.localeCompare(b.name, 'zh')
    })
}

export async function sftpStat(id: string, path: string): Promise<FileMeta> {
  const { sftp } = requireLive(id)
  const attrs = await new Promise<{ size: number; mtime: number; mode: number }>((resolve, reject) => {
    sftp.stat(path, (err, stats) => {
      if (err) reject(err)
      else resolve(stats)
    })
  })
  const name = path.split('/').filter(Boolean).pop() || path
  return toMeta(path.replace(/\/[^/]+$/, '') || '/', name, attrs)
}

export async function sftpRead(id: string, path: string, maxBytes?: number): Promise<Buffer> {
  const { sftp } = requireLive(id)
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = []
    let total = 0
    const stream = sftp.createReadStream(path)
    stream.on('data', (chunk: Buffer) => {
      total += chunk.length
      if (maxBytes && total > maxBytes) {
        stream.destroy()
        reject(new Error('FILE_TOO_LARGE'))
        return
      }
      chunks.push(chunk)
    })
    stream.on('error', reject)
    stream.on('end', () => resolve(Buffer.concat(chunks)))
  })
}

export async function sftpWrite(id: string, path: string, data: Buffer): Promise<void> {
  const { sftp } = requireLive(id)
  await new Promise<void>((resolve, reject) => {
    const stream = sftp.createWriteStream(path)
    stream.on('error', reject)
    stream.on('close', () => resolve())
    stream.end(data)
  })
}

export async function sftpMkdir(id: string, path: string): Promise<void> {
  const { sftp } = requireLive(id)
  const parts = path.split('/').filter(Boolean)
  let cur = path.startsWith('/') ? '' : ''
  for (const part of parts) {
    cur += `/${part}`
    await new Promise<void>((resolve, reject) => {
      sftp.mkdir(cur, (err) => {
        if (err && (err as NodeJS.ErrnoException).code !== 4) {
          sftp.stat(cur, (statErr, stats) => {
            if (statErr) reject(err)
            else if (isDirMode(stats.mode)) resolve()
            else reject(err)
          })
        } else resolve()
      })
    })
  }
}

export async function sftpRemove(id: string, path: string, isDir: boolean): Promise<void> {
  const { sftp } = requireLive(id)
  if (!isDir) {
    await new Promise<void>((resolve, reject) => {
      sftp.unlink(path, (err) => (err ? reject(err) : resolve()))
    })
    return
  }
  const children = await sftpList(id, path)
  for (const child of children) {
    await sftpRemove(id, child.path, child.type === 'dir')
  }
  await new Promise<void>((resolve, reject) => {
    sftp.rmdir(path, (err) => (err ? reject(err) : resolve()))
  })
}

export async function sftpExists(id: string, path: string): Promise<boolean> {
  try {
    await sftpStat(id, path)
    return true
  } catch {
    return false
  }
}

export async function disconnectAll(): Promise<void> {
  const ids = [...POOL.keys()]
  await Promise.all(ids.map(disconnectSftp))
}
