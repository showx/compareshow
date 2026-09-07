import { existsSync, readFileSync } from 'fs'
import { homedir, userInfo } from 'os'
import { join } from 'path'

export interface ResolvedSsh {
  host: string
  port: number
  username: string
  identityFiles: string[]
}

interface HostBlock {
  patterns: string[]
  hostName?: string
  user?: string
  port?: number
  identityFiles: string[]
}

export function sshDir(): string {
  return join(homedir(), '.ssh')
}

export function agentPath(): string | undefined {
  if (process.platform === 'win32') {
    const pipe = '\\\\.\\pipe\\openssh-ssh-agent'
    return existsSync(pipe) ? pipe : undefined
  }
  const sock = process.env.SSH_AUTH_SOCK
  return sock && existsSync(sock) ? sock : undefined
}

export function defaultIdentityFiles(): string[] {
  const dir = sshDir()
  return ['id_ed25519', 'id_ecdsa', 'id_rsa', 'id_dsa']
    .map((name) => join(dir, name))
    .filter((p) => existsSync(p))
}

export function resolveSshTarget(host: string, username?: string, port?: number): ResolvedSsh {
  const blocks = loadSshConfig()
  const matched = blocks.filter((b) => b.patterns.some((p) => matchHost(p, host)))
  const hostName = first(matched, (b) => b.hostName) || host
  const user = username || first(matched, (b) => b.user) || userInfo().username || 'root'
  const resolvedPort = port || first(matched, (b) => b.port) || 22
  const identityFiles = unique([
    ...matched.flatMap((b) => b.identityFiles),
    ...defaultIdentityFiles()
  ].map(expandPath).filter((p) => existsSync(p)))

  return { host: hostName, port: resolvedPort, username: user, identityFiles }
}

function first<T, R>(items: T[], pick: (item: T) => R | undefined): R | undefined {
  for (const item of items) {
    const value = pick(item)
    if (value !== undefined && value !== '') return value
  }
  return undefined
}

function unique(items: string[]): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const item of items) {
    const key = item.toLowerCase()
    if (seen.has(key)) continue
    seen.add(key)
    out.push(item)
  }
  return out
}

function expandPath(p: string): string {
  if (p.startsWith('~/') || p.startsWith('~\\')) return join(homedir(), p.slice(2))
  if (p === '~') return homedir()
  return p.replace(/%d/g, homedir()).replace(/%u/g, userInfo().username)
}

function matchHost(pattern: string, host: string): boolean {
  const negated = pattern.startsWith('!')
  const p = negated ? pattern.slice(1) : pattern
  const re = new RegExp('^' + p.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.') + '$', 'i')
  const ok = re.test(host)
  return negated ? !ok : ok
}

function loadSshConfig(): HostBlock[] {
  const file = join(sshDir(), 'config')
  if (!existsSync(file)) return []
  let text = ''
  try {
    text = readFileSync(file, 'utf8')
  } catch {
    return []
  }

  const blocks: HostBlock[] = []
  let current: HostBlock | null = null
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.replace(/#.*$/, '').trim()
    if (!line) continue
    const sep = line.search(/\s|=/)
    if (sep < 0) continue
    const key = line.slice(0, sep).toLowerCase()
    const value = line.slice(sep + 1).trim().replace(/^"|"$/g, '')
    if (key === 'host') {
      current = { patterns: value.split(/\s+/).filter(Boolean), identityFiles: [] }
      blocks.push(current)
      continue
    }
    if (!current) continue
    if (key === 'hostname') current.hostName = value
    else if (key === 'user') current.user = value
    else if (key === 'port') current.port = Number(value) || 22
    else if (key === 'identityfile') current.identityFiles.push(value)
  }
  return blocks
}

export function isAuthFailure(err: unknown): boolean {
  const m = (err instanceof Error ? err.message : String(err)).toLowerCase()
  return (
    m.includes('auth') ||
    m.includes('permission denied') ||
    m.includes('unable to authenticate') ||
    m.includes('all configured authentication methods failed') ||
    m.includes('encrypted private') ||
    m.includes('passphrase') ||
    m.includes('no matching') ||
    m.includes('login')
  )
}

export function isHostUnreachable(err: unknown): boolean {
  const m = (err instanceof Error ? err.message : String(err)).toLowerCase()
  const code = (err as NodeJS.ErrnoException).code || ''
  return (
    code === 'ECONNREFUSED' ||
    code === 'ENOTFOUND' ||
    code === 'EHOSTUNREACH' ||
    code === 'ETIMEDOUT' ||
    m.includes('timed out') ||
    m.includes('timeout') ||
    m.includes('econnrefused') ||
    m.includes('enotfound') ||
    m.includes('ehostunreach')
  )
}

export class SshConnectError extends Error {
  constructor(
    message: string,
    readonly code: 'AUTH' | 'HOST' | 'UNKNOWN',
    readonly keysTried: string[] = []
  ) {
    super(message)
    this.name = 'SshConnectError'
  }
}

export function isAgentFailure(err: unknown): boolean {
  const m = (err instanceof Error ? err.message : String(err)).toLowerCase()
  const code = (err as NodeJS.ErrnoException).code || ''
  return (
    code === 'ENOENT' ||
    m.includes('agent') ||
    m.includes('pageant') ||
    m.includes('named pipe')
  )
}
