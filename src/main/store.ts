import { app } from 'electron'
import { safeStorage } from 'electron'
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs'
import { join } from 'path'
import type { RecentSession, SftpConnectionConfig } from '../shared/types'

interface StoreShape {
  connections: StoredConnection[]
  recent: RecentSession[]
}

interface StoredConnection extends Omit<SftpConnectionConfig, 'password' | 'passphrase'> {
  passwordEnc?: string
  passphraseEnc?: string
}

export type { RecentSession }

function storePath(): string {
  const dir = app.getPath('userData')
  if (!existsSync(dir)) mkdirSync(dir, { recursive: true })
  return join(dir, 'sessions.json')
}

function loadRaw(): StoreShape {
  try {
    const raw = readFileSync(storePath(), 'utf8')
    return JSON.parse(raw) as StoreShape
  } catch {
    return { connections: [], recent: [] }
  }
}

function saveRaw(data: StoreShape): void {
  writeFileSync(storePath(), JSON.stringify(data, null, 2), 'utf8')
}

function encrypt(value?: string): string | undefined {
  if (!value) return undefined
  if (!safeStorage.isEncryptionAvailable()) {
    return Buffer.from(value, 'utf8').toString('base64')
  }
  return safeStorage.encryptString(value).toString('base64')
}

function decrypt(value?: string): string | undefined {
  if (!value) return undefined
  try {
    const buf = Buffer.from(value, 'base64')
    if (!safeStorage.isEncryptionAvailable()) {
      return buf.toString('utf8')
    }
    return safeStorage.decryptString(buf)
  } catch {
    return undefined
  }
}

export function listConnections(): SftpConnectionConfig[] {
  return loadRaw().connections.map(toPublic)
}

export function upsertConnection(input: SftpConnectionConfig): SftpConnectionConfig {
  const data = loadRaw()
  const stored: StoredConnection = {
    id: input.id,
    name: input.name,
    host: input.host,
    port: input.port,
    username: input.username,
    authType: input.authType,
    privateKeyPath: input.privateKeyPath,
    defaultPath: input.defaultPath,
    createdAt: input.createdAt,
    lastUsedAt: input.lastUsedAt,
    passwordEnc: encrypt(input.password),
    passphraseEnc: encrypt(input.passphrase)
  }
  const idx = data.connections.findIndex((c) => c.id === input.id)
  if (idx >= 0) data.connections[idx] = stored
  else data.connections.unshift(stored)
  saveRaw(data)
  return toPublic(stored)
}

export function deleteConnection(id: string): void {
  const data = loadRaw()
  data.connections = data.connections.filter((c) => c.id !== id)
  saveRaw(data)
}

export function getConnection(id: string): SftpConnectionConfig | undefined {
  const found = loadRaw().connections.find((c) => c.id === id)
  return found ? toPublic(found) : undefined
}

export function findByEndpoint(username: string, host: string, port: number): SftpConnectionConfig | undefined {
  const u = username.toLowerCase()
  const h = host.toLowerCase()
  const found = loadRaw().connections.find(
    (c) => c.username.toLowerCase() === u && c.host.toLowerCase() === h && (c.port || 22) === (port || 22)
  )
  return found ? toPublic(found) : undefined
}

export function touchConnection(id: string): void {
  const data = loadRaw()
  const found = data.connections.find((c) => c.id === id)
  if (!found) return
  found.lastUsedAt = Date.now()
  saveRaw(data)
}

export function listRecent(): RecentSession[] {
  return loadRaw().recent.slice(0, 20)
}

export function addRecent(session: RecentSession): void {
  const data = loadRaw()
  data.recent = [
    session,
    ...data.recent.filter(
      (r) =>
        !(
          r.leftPath === session.leftPath &&
          r.rightPath === session.rightPath &&
          r.leftConnectionId === session.leftConnectionId &&
          r.rightConnectionId === session.rightConnectionId
        )
    )
  ].slice(0, 20)
  saveRaw(data)
}

function toPublic(stored: StoredConnection): SftpConnectionConfig {
  return {
    id: stored.id,
    name: stored.name,
    host: stored.host,
    port: stored.port,
    username: stored.username,
    authType: stored.authType,
    privateKeyPath: stored.privateKeyPath,
    defaultPath: stored.defaultPath,
    createdAt: stored.createdAt,
    lastUsedAt: stored.lastUsedAt,
    password: decrypt(stored.passwordEnc),
    passphrase: decrypt(stored.passphraseEnc)
  }
}
