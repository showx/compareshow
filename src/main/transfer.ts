import { basename } from 'path'
import { createHash } from 'crypto'
import type { LocationRef } from '../shared/types'
import * as local from './local-fs'
import * as sftp from './sftp'

export async function copyEntry(source: LocationRef, targetDir: LocationRef, isDir: boolean): Promise<void> {
  const name = source.kind === 'local' ? basename(source.path) : source.path.split('/').filter(Boolean).pop() || 'item'
  const destPath = targetDir.kind === 'local'
    ? local.joinLocal(targetDir.path, name)
    : local.posixJoin(targetDir.path, name)
  const dest: LocationRef = { ...targetDir, path: destPath }

  if (isDir) {
    await copyDir(source, dest)
  } else {
    await copyFile(source, dest)
  }
}

async function copyFile(source: LocationRef, dest: LocationRef): Promise<void> {
  if (source.kind === 'local' && dest.kind === 'local') {
    await local.localCopyFile(source.path, dest.path)
    return
  }
  const buf = source.kind === 'local'
    ? await local.localRead(source.path)
    : await sftp.sftpRead(source.connectionId!, source.path)
  if (dest.kind === 'local') {
    await local.localWrite(dest.path, buf)
  } else {
    const parent = dest.path.replace(/\/[^/]+$/, '') || '/'
    await sftp.sftpMkdir(dest.connectionId!, parent)
    await sftp.sftpWrite(dest.connectionId!, dest.path, buf)
  }
}

async function copyDir(source: LocationRef, dest: LocationRef): Promise<void> {
  if (dest.kind === 'local') await local.localMkdir(dest.path)
  else await sftp.sftpMkdir(dest.connectionId!, dest.path)

  const children = source.kind === 'local'
    ? await local.localList(source.path)
    : await sftp.sftpList(source.connectionId!, source.path)

  for (const child of children) {
    const nextSrc: LocationRef = {
      ...source,
      path: source.kind === 'local' ? local.joinLocal(source.path, child.name) : local.posixJoin(source.path, child.name)
    }
    const nextDest: LocationRef = {
      ...dest,
      path: dest.kind === 'local' ? local.joinLocal(dest.path, child.name) : local.posixJoin(dest.path, child.name)
    }
    if (child.type === 'dir') await copyDir(nextSrc, nextDest)
    else await copyFile(nextSrc, nextDest)
  }
}

export async function removeEntry(loc: LocationRef, isDir: boolean): Promise<void> {
  if (loc.kind === 'local') {
    await local.localRemove(loc.path)
    return
  }
  await sftp.sftpRemove(loc.connectionId!, loc.path, isDir)
}

export function hashBuffer(buf: Buffer): string {
  return createHash('md5').update(buf).digest('hex')
}

export function isProbablyBinary(buf: Buffer): boolean {
  const sample = buf.subarray(0, Math.min(buf.length, 8192))
  if (sample.includes(0)) return true
  let weird = 0
  for (const b of sample) {
    if (b < 7 || (b > 14 && b < 32 && b !== 9 && b !== 10 && b !== 13)) weird++
  }
  return weird / sample.length > 0.3
}

export function decodeText(buf: Buffer): { text: string; encoding: string } {
  if (buf.length >= 2 && buf[0] === 0xff && buf[1] === 0xfe) {
    return { text: buf.subarray(2).toString('utf16le'), encoding: 'utf-16le' }
  }
  if (buf.length >= 3 && buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf) {
    return { text: buf.subarray(3).toString('utf8'), encoding: 'utf-8' }
  }
  return { text: buf.toString('utf8'), encoding: 'utf-8' }
}
