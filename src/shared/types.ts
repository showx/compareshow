export type LocationKind = 'local' | 'sftp'
export type AuthType = 'password' | 'privateKey'
export type CompareMode = 'quick' | 'content'
export type ItemStatus = 'same' | 'different' | 'leftOnly' | 'rightOnly' | 'conflict' | 'unknown'
export type EntryType = 'file' | 'dir'

export interface SftpConnectionConfig {
  id: string
  name: string
  host: string
  port: number
  username: string
  authType: AuthType
  password?: string
  privateKeyPath?: string
  passphrase?: string
  defaultPath?: string
  createdAt: number
  lastUsedAt: number
}

export interface LocationRef {
  kind: LocationKind
  path: string
  connectionId?: string
  displayUrl?: string
}

export interface FileMeta {
  name: string
  path: string
  type: EntryType
  size: number
  mtime: number
  mode?: number
}

export interface CompareNode {
  name: string
  relPath: string
  type: EntryType
  status: ItemStatus
  left?: FileMeta
  right?: FileMeta
  children?: CompareNode[]
}

export interface CompareProgress {
  scanned: number
  currentPath: string
}

export interface FolderCompareRequest {
  left: LocationRef
  right: LocationRef
  mode: CompareMode
  ignorePatterns: string[]
}

export interface FolderCompareResult {
  tree: CompareNode[]
  stats: CompareStats
}

export interface CompareStats {
  same: number
  different: number
  leftOnly: number
  rightOnly: number
  files: number
  dirs: number
}

export interface FileCompareRequest {
  left: LocationRef
  right: LocationRef
}

export interface FileCompareResult {
  leftMeta?: FileMeta
  rightMeta?: FileMeta
  leftText?: string
  rightText?: string
  leftBinary?: boolean
  rightBinary?: boolean
  leftHash?: string
  rightHash?: string
  identical: boolean
  tooLarge: boolean
  encoding?: string
}

export interface TransferRequest {
  source: LocationRef
  target: LocationRef
  isDir: boolean
}

export interface SessionTab {
  id: string
  kind: 'folder' | 'file'
  title: string
  left: LocationRef
  right: LocationRef
}

export interface RecentSession {
  id: string
  title: string
  leftKind: LocationKind
  rightKind: LocationKind
  leftPath: string
  rightPath: string
  leftConnectionId?: string
  rightConnectionId?: string
  at: number
}

export const TEXT_MAX_BYTES = 8 * 1024 * 1024
