import { contextBridge, ipcRenderer } from 'electron'
import type {
  FileCompareRequest,
  FileCompareResult,
  FileMeta,
  FolderCompareRequest,
  FolderCompareResult,
  LocationRef,
  RecentSession,
  SftpConnectionConfig
} from '../shared/types'

type Result<T> = { ok: true; data: T } | { ok: false; error: string }

const api = {
  connections: {
    list: () => ipcRenderer.invoke('connections:list') as Promise<SftpConnectionConfig[]>,
    get: (id: string) => ipcRenderer.invoke('connections:get', id) as Promise<SftpConnectionConfig | null>,
    save: (c: SftpConnectionConfig) => ipcRenderer.invoke('connections:save', c) as Promise<SftpConnectionConfig>,
    delete: (id: string) => ipcRenderer.invoke('connections:delete', id) as Promise<void>
  },
  recent: {
    list: () => ipcRenderer.invoke('recent:list') as Promise<RecentSession[]>,
    add: (session: RecentSession) => ipcRenderer.invoke('recent:add', session) as Promise<void>
  },
  sftp: {
    test: (c: SftpConnectionConfig) => ipcRenderer.invoke('sftp:test', c) as Promise<Result<boolean>>,
    connect: (id: string) => ipcRenderer.invoke('sftp:connect', id) as Promise<Result<boolean>>,
    disconnect: (id: string) => ipcRenderer.invoke('sftp:disconnect', id) as Promise<void>,
    connected: (id: string) => ipcRenderer.invoke('sftp:connected', id) as Promise<boolean>,
    list: (id: string, path: string) => ipcRenderer.invoke('sftp:list', id, path) as Promise<Result<FileMeta[]>>,
    openUrl: (url: string, password?: string, remember?: boolean) =>
      ipcRenderer.invoke('sftp:openUrl', url, password, remember) as Promise<
        | { ok: true; data: { connectionId: string; path: string; displayUrl: string; connection: SftpConnectionConfig } }
        | { ok: false; error: string; needPassword?: boolean; username?: string; host?: string; port?: number; keysTried?: string[] }
      >
  },
  local: {
    list: (path: string) => ipcRenderer.invoke('local:list', path) as Promise<Result<FileMeta[]>>,
    resolve: (path: string) =>
      ipcRenderer.invoke('local:resolve', path) as Promise<Result<{ path: string; type: 'dir' | 'file' | 'drives' }>>,
    exists: (path: string) => ipcRenderer.invoke('local:exists', path) as Promise<boolean>
  },
  dialog: {
    openDirectory: (title?: string) => ipcRenderer.invoke('dialog:openDirectory', title) as Promise<string | null>,
    openFile: (title?: string) => ipcRenderer.invoke('dialog:openFile', title) as Promise<string | null>
  },
  compare: {
    folder: (req: FolderCompareRequest) =>
      ipcRenderer.invoke('compare:folder', req) as Promise<Result<FolderCompareResult>>,
    file: (req: FileCompareRequest) =>
      ipcRenderer.invoke('compare:file', req) as Promise<Result<FileCompareResult>>,
    onProgress: (cb: (p: { scanned: number; currentPath: string }) => void) => {
      const listener = (_e: unknown, p: { scanned: number; currentPath: string }) => cb(p)
      ipcRenderer.on('compare:progress', listener)
      return () => ipcRenderer.removeListener('compare:progress', listener)
    }
  },
  transfer: {
    copy: (source: LocationRef, targetDir: LocationRef, isDir: boolean) =>
      ipcRenderer.invoke('transfer:copy', source, targetDir, isDir) as Promise<Result<void>>,
    remove: (loc: LocationRef, isDir: boolean) =>
      ipcRenderer.invoke('transfer:remove', loc, isDir) as Promise<Result<void>>
  },
  shell: {
    show: (path: string) => ipcRenderer.invoke('shell:show', path) as Promise<void>
  }
}

export type CompareShowApi = typeof api

function expose(): void {
  try {
    if (process.contextIsolated) {
      contextBridge.exposeInMainWorld('api', api)
    } else {
      ;(globalThis as unknown as { api: CompareShowApi }).api = api
    }
  } catch (err) {
    console.error('[preload] expose api failed', err)
    try {
      ;(globalThis as unknown as { api: CompareShowApi }).api = api
    } catch {
      /* ignore */
    }
  }
}

expose()
