import { app, BrowserWindow, dialog, ipcMain, shell } from 'electron'
import { existsSync } from 'fs'
import { join } from 'path'
import { randomUUID } from 'crypto'
import type {
  FileCompareRequest,
  FileCompareResult,
  FolderCompareRequest,
  LocationRef,
  SftpConnectionConfig
} from '../shared/types'
import { TEXT_MAX_BYTES } from '../shared/types'
import { formatSftpUrl, parseSftpUrl } from '../shared/sftp-url'
import { isAuthFailure, resolveSshTarget, SshConnectError } from './ssh-auth'
import * as store from './store'
import * as sftp from './sftp'
import { compareFolders, readForCompare } from './compare'
import { copyEntry, decodeText, hashBuffer, isProbablyBinary, removeEntry } from './transfer'
import { localExists, localList, localResolve } from './local-fs'

let mainWindow: BrowserWindow | null = null

function resolvePreload(): string {
  const candidates = [
    join(__dirname, '../preload/index.js'),
    join(__dirname, '../preload/index.mjs'),
    join(__dirname, '../preload/index.cjs')
  ]
  return candidates.find((p) => existsSync(p)) || candidates[0]
}

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 920,
    minWidth: 1100,
    minHeight: 700,
    show: false,
    backgroundColor: '#0e1116',
    title: 'CompareShow',
    autoHideMenuBar: true,
    titleBarStyle: 'hidden',
    titleBarOverlay: {
      color: '#0e1116',
      symbolColor: '#c5cad3',
      height: 40
    },
    webPreferences: {
      preload: resolvePreload(),
      sandbox: false,
      contextIsolation: true,
      nodeIntegration: false
    }
  })

  mainWindow.webContents.on('preload-error', (_event, path, error) => {
    console.error('[preload-error]', path, error)
    void dialog.showErrorBox('预加载失败', `${error.message}\n${path}`)
  })

  mainWindow.on('ready-to-show', () => mainWindow?.show())
  mainWindow.webContents.setWindowOpenHandler((details) => {
    shell.openExternal(details.url)
    return { action: 'deny' }
  })

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL)
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

function sendProgress(scanned: number, currentPath: string): void {
  mainWindow?.webContents.send('compare:progress', { scanned, currentPath })
}

function wrap<T>(fn: () => Promise<T>): Promise<{ ok: true; data: T } | { ok: false; error: string }> {
  return fn()
    .then((data) => ({ ok: true as const, data }))
    .catch((err: unknown) => ({
      ok: false as const,
      error: err instanceof Error ? err.message : String(err)
    }))
}

function registerIpc(): void {
  ipcMain.handle('connections:list', () => store.listConnections().map(stripSecrets))
  ipcMain.handle('connections:get', (_e, id: string) => {
    const c = store.getConnection(id)
    return c ? stripSecrets(c) : null
  })
  ipcMain.handle('connections:save', (_e, input: SftpConnectionConfig) => {
    const full = hydrateConfig(input)
    const saved = store.upsertConnection({
      ...full,
      name: input.name || full.name,
      createdAt: full.createdAt || Date.now(),
      lastUsedAt: Date.now()
    })
    return stripSecrets(saved)
  })
  ipcMain.handle('connections:delete', (_e, id: string) => {
    void sftp.disconnectSftp(id)
    store.deleteConnection(id)
  })
  ipcMain.handle('recent:list', () => store.listRecent())
  ipcMain.handle('recent:add', (_e, session: store.RecentSession) => {
    store.addRecent({ ...session, id: session.id || randomUUID(), at: Date.now() })
  })

  ipcMain.handle('sftp:test', (_e, config: SftpConnectionConfig) =>
    wrap(async () => {
      const full = hydrateConfig(config)
      await sftp.connectSftp(full)
      store.touchConnection(full.id)
      return true
    })
  )
  ipcMain.handle('sftp:connect', (_e, id: string) =>
    wrap(async () => {
      const cfg = store.getConnection(id)
      if (!cfg) throw new Error('找不到该连接')
      await sftp.connectSftp(cfg)
      store.touchConnection(id)
      return true
    })
  )
  ipcMain.handle('sftp:disconnect', (_e, id: string) => sftp.disconnectSftp(id))
  ipcMain.handle('sftp:connected', (_e, id: string) => sftp.hasConnection(id))
  ipcMain.handle('sftp:list', (_e, id: string, path: string) => wrap(() => sftp.sftpList(id, path)))
  ipcMain.handle('sftp:openUrl', (_e, url: string, password?: string, remember = true) => openSftpUrl(url, password, remember))

  ipcMain.handle('local:list', (_e, path: string) => wrap(() => localList(path)))
  ipcMain.handle('local:resolve', (_e, path: string) => wrap(() => localResolve(path)))
  ipcMain.handle('local:exists', (_e, path: string) => localExists(path))

  ipcMain.handle('dialog:openDirectory', async (_e, title?: string) => {
    if (!mainWindow) return null
    const res = await dialog.showOpenDialog(mainWindow, {
      title: title || '选择文件夹',
      properties: ['openDirectory']
    })
    return res.canceled ? null : res.filePaths[0]
  })
  ipcMain.handle('dialog:openFile', async (_e, title?: string) => {
    if (!mainWindow) return null
    const res = await dialog.showOpenDialog(mainWindow, {
      title: title || '选择文件',
      properties: ['openFile']
    })
    return res.canceled ? null : res.filePaths[0]
  })

  ipcMain.handle('compare:folder', (_e, req: FolderCompareRequest) =>
    wrap(async () => {
      await ensureConnected(req.left)
      await ensureConnected(req.right)
      return compareFolders(req, sendProgress)
    })
  )

  ipcMain.handle('compare:file', (_e, req: FileCompareRequest) =>
    wrap(async (): Promise<FileCompareResult> => {
      await ensureConnected(req.left)
      await ensureConnected(req.right)
      const [l, r] = await Promise.all([
        readForCompare(req.left, TEXT_MAX_BYTES),
        readForCompare(req.right, TEXT_MAX_BYTES)
      ])
      if (l.tooLarge || r.tooLarge) {
        return {
          leftMeta: l.meta,
          rightMeta: r.meta,
          identical: false,
          tooLarge: true,
          leftBinary: true,
          rightBinary: true
        }
      }
      const leftBinary = l.buf ? isProbablyBinary(l.buf) : false
      const rightBinary = r.buf ? isProbablyBinary(r.buf) : false
      const leftHash = l.buf ? hashBuffer(l.buf) : undefined
      const rightHash = r.buf ? hashBuffer(r.buf) : undefined
      const identical = Boolean(leftHash && rightHash && leftHash === rightHash)
      const result: FileCompareResult = {
        leftMeta: l.meta,
        rightMeta: r.meta,
        leftBinary,
        rightBinary,
        leftHash,
        rightHash,
        identical,
        tooLarge: false
      }
      if (!leftBinary && l.buf) {
        const d = decodeText(l.buf)
        result.leftText = d.text
        result.encoding = d.encoding
      }
      if (!rightBinary && r.buf) {
        const d = decodeText(r.buf)
        result.rightText = d.text
      }
      return result
    })
  )

  ipcMain.handle('transfer:copy', (_e, source: LocationRef, targetDir: LocationRef, isDir: boolean) =>
    wrap(async () => {
      await ensureConnected(source)
      await ensureConnected(targetDir)
      await copyEntry(source, targetDir, isDir)
    })
  )
  ipcMain.handle('transfer:remove', (_e, loc: LocationRef, isDir: boolean) =>
    wrap(async () => {
      await ensureConnected(loc)
      await removeEntry(loc, isDir)
    })
  )

  ipcMain.handle('shell:show', (_e, path: string) => shell.showItemInFolder(path))
}

function stripSecrets(c: SftpConnectionConfig): SftpConnectionConfig {
  return {
    ...c,
    password: c.password ? '********' : undefined,
    passphrase: c.passphrase ? '********' : undefined
  }
}

function hydrateConfig(config: SftpConnectionConfig): SftpConnectionConfig {
  const saved = config.id ? store.getConnection(config.id) : undefined
  const password = config.password === '********' ? saved?.password : config.password
  const passphrase = config.passphrase === '********' ? saved?.passphrase : config.passphrase
  return {
    ...config,
    id: config.id || randomUUID(),
    password,
    passphrase,
    createdAt: config.createdAt || saved?.createdAt || Date.now(),
    lastUsedAt: Date.now()
  }
}

async function ensureConnected(loc: LocationRef): Promise<void> {
  if (loc.kind !== 'sftp' || !loc.connectionId) return
  if (sftp.hasConnection(loc.connectionId)) return
  const cfg = store.getConnection(loc.connectionId)
  if (!cfg) throw new Error('SFTP 连接不存在，请先保存并测试连接')
  await sftp.connectSftp(cfg)
}

function isAuthError(err: unknown): boolean {
  return isAuthFailure(err)
}

async function openSftpUrl(url: string, password?: string, remember = true) {
  const parsed = parseSftpUrl(url)
  if (!parsed) {
    return { ok: false as const, error: '无法解析地址。请使用 ssh://root@主机 或 sftp://root@主机/路径' }
  }

  const resolved = resolveSshTarget(parsed.host, parsed.username, parsed.port)
  const username = parsed.username || resolved.username
  const host = resolved.host
  const port = parsed.port || resolved.port
  const path = parsed.path || '/'

  let cfg = store.findByEndpoint(username, host, port) || store.findByEndpoint(username, parsed.host, port)
  const pass = password || parsed.password || cfg?.password

  const now = Date.now()
  cfg = store.upsertConnection({
    id: cfg?.id || randomUUID(),
    name: cfg?.name || `${username}@${parsed.host}`,
    host: parsed.host,
    port,
    username,
    authType: pass ? 'password' : (cfg?.privateKeyPath ? 'privateKey' : 'password'),
    password: remember ? pass : cfg?.password,
    privateKeyPath: cfg?.privateKeyPath,
    passphrase: password && !parsed.password ? (cfg?.passphrase || password) : cfg?.passphrase,
    defaultPath: path,
    createdAt: cfg?.createdAt || now,
    lastUsedAt: now
  })

  try {
    await sftp.connectSftp({
      ...cfg,
      username,
      host: parsed.host,
      port,
      password: pass,
      passphrase: cfg.passphrase || (password && !parsed.password ? password : undefined)
    })
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    if (err instanceof SshConnectError && err.code === 'HOST') {
      return { ok: false as const, error: message }
    }
    if ((err instanceof SshConnectError && err.code === 'AUTH') || isAuthError(err)) {
      return {
        ok: false as const,
        error: message,
        needPassword: true,
        username,
        host: parsed.host,
        port,
        keysTried: err instanceof SshConnectError ? err.keysTried : []
      }
    }
    return { ok: false as const, error: message }
  }

  store.touchConnection(cfg.id)
  return {
    ok: true as const,
    data: {
      connectionId: cfg.id,
      path,
      displayUrl: formatSftpUrl({
        username,
        host: parsed.host,
        port,
        path,
        scheme: parsed.scheme === 'ssh' ? 'ssh' : 'sftp'
      }),
      connection: stripSecrets(cfg)
    }
  }
}

app.whenReady().then(() => {
  registerIpc()
  createWindow()
  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

app.on('window-all-closed', () => {
  void sftp.disconnectAll()
  if (process.platform !== 'darwin') app.quit()
})
