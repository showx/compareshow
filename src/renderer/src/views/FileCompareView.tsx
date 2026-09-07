import { useEffect, useRef, useState } from 'react'
import { DiffEditor, loader } from '@monaco-editor/react'
import * as monaco from 'monaco-editor'
import type { FileCompareResult, LocationRef } from '../../../shared/types'
import { formatSize, languageFromPath } from '../lib/format'
import { parentPath } from '../lib/paths'

loader.config({ monaco })

monaco.editor.defineTheme('cs-dark', {
  base: 'vs-dark',
  inherit: true,
  rules: [],
  colors: {
    'editor.background': '#0e1116',
    'editorGutter.background': '#0e1116',
    'diffEditor.insertedTextBackground': '#3ccf9128',
    'diffEditor.removedTextBackground': '#e85d5d28',
    'diffEditor.insertedLineBackground': '#3ccf910f',
    'diffEditor.removedLineBackground': '#e85d5d0f'
  }
})

interface Props {
  left: LocationRef
  right: LocationRef
  onToast: (msg: string, error?: boolean) => void
}

export default function FileCompareView({ left, right, onToast }: Props) {
  const [data, setData] = useState<FileCompareResult | null>(null)
  const [busy, setBusy] = useState(true)
  const [ignoreWs, setIgnoreWs] = useState(false)
  const [sideBySide, setSideBySide] = useState(true)
  const editorRef = useRef<monaco.editor.IStandaloneDiffEditor | null>(null)

  useEffect(() => {
    void (async () => {
      setBusy(true)
      const res = await window.api.compare.file({ left, right })
      setBusy(false)
      if (!res.ok) {
        onToast(res.error, true)
        return
      }
      setData(res.data)
    })()
  }, [left.path, right.path])

  async function copyFile(dir: 'toRight' | 'toLeft') {
    const source = dir === 'toRight' ? left : right
    const dest = dir === 'toRight' ? right : left
    if (!source.path || !dest.path) {
      onToast('这一侧文件不存在', true)
      return
    }
    const res = await window.api.transfer.copy(source, parentPath(dest), false)
    if (!res.ok) onToast(res.error, true)
    else onToast(dir === 'toRight' ? '已覆盖到右侧' : '已覆盖到左侧')
  }

  if (busy) return <div className="empty">正在读取文件…</div>
  if (!data) return <div className="empty">无法比较这两个文件。</div>

  const binary = data.leftBinary || data.rightBinary || data.tooLarge
  const lang = languageFromPath(left.path || right.path)

  return (
    <div className="file-view">
      <div className="toolbar">
        <span style={{ fontSize: 12, color: 'var(--muted)' }}>
          {data.identical ? '内容相同' : binary ? '二进制或超大文件' : '文本差异'}
        </span>
        <div className="sep" />
        {!binary && (
          <>
            <button className="ghost-btn" onClick={() => editorRef.current?.goToDiff('previous')}>上一处</button>
            <button className="ghost-btn" onClick={() => editorRef.current?.goToDiff('next')}>下一处</button>
            <button className={`icon-btn ${sideBySide ? 'active' : ''}`} onClick={() => setSideBySide((v) => !v)}>
              {sideBySide ? '并排' : '内联'}
            </button>
            <button className={`icon-btn ${ignoreWs ? 'active' : ''}`} onClick={() => setIgnoreWs((v) => !v)}>忽略空白</button>
          </>
        )}
        <div className="sep" />
        <button className="ghost-btn" disabled={!left.path} onClick={() => void copyFile('toRight')}>复制到右 →</button>
        <button className="ghost-btn" disabled={!right.path} onClick={() => void copyFile('toLeft')}>← 复制到左</button>
        <div className="grow" />
        <span style={{ fontSize: 11, color: 'var(--faint)' }}>{data.encoding || ''}</span>
      </div>
      <div className="file-meta">
        <div>{left.path || '（不存在）'}{data.leftMeta ? `  ·  ${formatSize(data.leftMeta.size)}` : ''}</div>
        <div>{right.path || '（不存在）'}{data.rightMeta ? `  ·  ${formatSize(data.rightMeta.size)}` : ''}</div>
      </div>
      {binary ? (
        <div className="binary-panel">
          <div>
            <h3>{data.identical ? '两侧文件内容一致' : data.tooLarge ? '文件过大，已跳过全文加载' : '二进制文件'}</h3>
            <p>左侧 MD5：<span className="hash">{data.leftHash || '—'}</span></p>
            <p>右侧 MD5：<span className="hash">{data.rightHash || '—'}</span></p>
          </div>
        </div>
      ) : (
        <div className="diff-host">
          <DiffEditor
            original={data.leftText ?? ''}
            modified={data.rightText ?? ''}
            language={lang}
            theme="cs-dark"
            onMount={(editor) => { editorRef.current = editor }}
            options={{
              readOnly: true,
              renderSideBySide: sideBySide,
              ignoreTrimWhitespace: ignoreWs,
              minimap: { enabled: false },
              fontSize: 13,
              fontFamily: 'Cascadia Code, Consolas, monospace',
              scrollBeyondLastLine: false,
              automaticLayout: true,
              originalEditable: false,
              renderIndicators: true
            }}
          />
        </div>
      )}
    </div>
  )
}
