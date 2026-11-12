import { useEffectEvent, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Annotation, Compartment, EditorState, Transaction } from '@codemirror/state'
import { Decoration, EditorView, ViewPlugin, WidgetType, keymap, placeholder, type ViewUpdate } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands'
import { markdown } from '@codemirror/lang-markdown'
import { syntaxTree } from '@codemirror/language'
import './MarkdownEditor.css'

export function normalizeLegacyImages(value: string): string {
  return value.replace(/\[图片:\s*([^\]]+)\]\s*\((file:\/\/\/[^)\r\n]+)\)/gi, (original, label, uri) => {
    try {
      const match = /\/writing-assets\/(.+)$/i.exec(new URL(uri).pathname)
      if (!match) return original
      const relative = decodeURIComponent(match[1]).split(/[\\/]+/).filter(Boolean)
        .map(segment => encodeURIComponent(segment).replace(/[!'()*]/g, char => '%' + char.charCodeAt(0).toString(16).toUpperCase()))
        .join('/')
      const alt = String(label).trim().replace(/\\/g, '\\\\').replace(/\]/g, '\\]')
      return '![' + alt + '](writing-assets/' + relative + ')'
    } catch { return original }
  })
}

class ImageWidget extends WidgetType {
  readonly path: string
  readonly alt: string
  constructor(path: string, alt: string) {
    super()
    this.path = path
    this.alt = alt
  }
  eq(other: ImageWidget) { return other.path === this.path && other.alt === this.alt }
  toDOM() {
    const wrapper = document.createElement('span')
    wrapper.className = 'inline-writing-image'
    const image = document.createElement('img')
    image.alt = this.alt
    image.decoding = 'async'
    wrapper.appendChild(image)
    void invoke<string>('writing_image_read', { reference: this.path }).then(src => {
      image.src = src
    }).catch(() => { wrapper.textContent = '图片无法加载：' + this.alt })
    return wrapper
  }
  ignoreEvent() { return false }
}

const managedImage = /^!\[((?:\\.|[^\]\\])*)\]\(((?:writing-assets\/[A-Za-z0-9_./%-]+|data\/picture\/[0-9a-f-]{36}\/[A-Za-z0-9_.%-]+))\)$/i
const markerStyle = Decoration.mark({ class: 'writing-marker' })

function presentMarkdown(view: EditorView) {
  const { state, visibleRanges } = view
  const lines = new Map<number, ReturnType<Decoration['range']>>()
  const marks = new Map<number, ReturnType<Decoration['range']>>()
  const images = new Map<number, ReturnType<Decoration['range']>>()
  for (const { from, to } of visibleRanges) {
    const firstVisible = state.doc.lineAt(from).number
    const lastVisible = state.doc.lineAt(to).number
    syntaxTree(state).iterate({ from, to, enter(ref) {
      const heading = /^(?:ATX|Setext)Heading([1-6])$/.exec(ref.name)
      if (heading) {
        const line = state.doc.lineAt(ref.from)
        lines.set(line.from, Decoration.line({ class: 'writing-heading writing-heading-' + heading[1] }).range(line.from))
      } else if (ref.name === 'FencedCode') {
        const start = state.doc.lineAt(ref.from).number
        const end = state.doc.lineAt(ref.to).number
        for (let i = Math.max(start, firstVisible); i <= Math.min(end, lastVisible); i += 1) {
          const line = state.doc.line(i)
          const classes = 'writing-code' + (i === start ? ' writing-code-start' : '') + (i === end ? ' writing-code-end' : '')
          lines.set(line.from, Decoration.line({ class: classes }).range(line.from))
        }
      } else if (ref.name === 'HeaderMark' || ref.name === 'CodeMark' || ref.name === 'CodeInfo') {
        marks.set(ref.from, markerStyle.range(ref.from, ref.to))
      } else if (ref.name === 'Image') {
        const image = managedImage.exec(state.sliceDoc(ref.from, ref.to))
        if (image) {
          const alt = image[1].replace(/\\([\\\]])/g, '$1') || '图片'
          images.set(ref.from, Decoration.replace({ widget: new ImageWidget(image[2], alt) }).range(ref.from, ref.to))
        }
      }
    } })
  }
  return {
    visible: Decoration.set([...lines.values(), ...marks.values(), ...images.values()], true),
    atomic: Decoration.set([...images.values()], true),
  }
}

const liveMarkdown = ViewPlugin.fromClass(class {
  presentation: ReturnType<typeof presentMarkdown>
  constructor(view: EditorView) { this.presentation = presentMarkdown(view) }
  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
      this.presentation = presentMarkdown(update.view)
    }
  }
}, {
  decorations: plugin => plugin.presentation.visible,
  provide: plugin => EditorView.atomicRanges.of(view => view.plugin(plugin)?.presentation.atomic ?? Decoration.none),
})

function completeFence(view: EditorView) {
  const { state } = view
  const cursor = state.selection.main
  if (state.readOnly || view.composing || !cursor.empty || state.selection.ranges.length !== 1) return false
  const line = state.doc.lineAt(cursor.head)
  if (line.to !== cursor.head) return false
  const fence = /^( {0,3})(`{3,}|~{3,})([^`~]*)$/.exec(line.text)
  if (!fence) return false
  let node = syntaxTree(state).resolveInner(line.from + fence[1].length, 1)
  while (node.parent && node.name !== 'FencedCode') node = node.parent
  if (node.name !== 'FencedCode' || node.from !== line.from + fence[1].length || node.getChildren('CodeMark').length !== 1) return false
  view.dispatch({
    changes: { from: cursor.head, insert: '\n' + fence[1] + '\n' + fence[1] + fence[2] },
    selection: { anchor: cursor.head + 1 + fence[1].length },
    userEvent: 'input',
    scrollIntoView: true,
  })
  return true
}

const externalValue = Annotation.define<boolean>()
const editableState = (editable: boolean) => [EditorView.editable.of(editable), EditorState.readOnly.of(!editable)]
const writingHint = '开始写作。用 # 写标题，三个反引号加回车插入代码块，Ctrl+V 粘贴图片。'

type Props = {
  documentId: string
  value: string
  onChange: (value: string) => void
  onPendingInput?: (task: Promise<boolean>) => void
  disabled?: boolean
  label: string
}

export default function MarkdownEditor({ documentId, value, onChange, onPendingInput, disabled = false, label }: Props) {
  const parent = useRef<HTMLDivElement>(null)
  const editor = useRef<EditorView | null>(null)
  const locked = useRef(false)
  const editability = useMemo(() => new Compartment(), [])
  const [imageError, setImageError] = useState('')
  const [importing, setImporting] = useState(false)
  const documentValue = useMemo(() => normalizeLegacyImages(value), [value])
  const initialState = useEffectEvent(() => ({ documentValue, disabled }))
  const changed = useEffectEvent((next: string) => onChange(next))
  const updateLock = useEffectEvent((view: EditorView, lock: boolean) => {
    locked.current = lock
    setImporting(lock)
    view.dispatch({ effects: editability.reconfigure(editableState(!lock && !disabled)) })
  })

  const pasteImage = useEffectEvent((view: EditorView, file: File) => {
    if (locked.current || disabled) return
    if (file.size > 8 * 1024 * 1024) { setImageError('图片不能超过 8 MiB'); return }
    const { from, to } = view.state.selection.main
    setImageError('')
    updateLock(view, true)
    const task = (async () => {
      try {
        const bytes = Array.from(new Uint8Array(await file.arrayBuffer()))
        const path = await invoke<string>('writing_image_paste', { documentId, bytes })
        if (editor.current !== view) return false
        const leading = from === view.state.doc.lineAt(from).from ? '' : '\n'
        const trailing = view.state.sliceDoc(to, to + 1) === '\n' ? '' : '\n'
        const image = leading + '![图片](' + path + ')'
        view.dispatch({
          changes: { from, to, insert: image + trailing },
          selection: { anchor: from + image.length + 1 },
          userEvent: 'input.paste',
          scrollIntoView: true,
        })
        return true
      } catch (error) {
        if (editor.current === view) setImageError('插入图片失败：' + String(error))
        return false
      } finally {
        if (editor.current === view) {
          updateLock(view, false)
          if (view.hasFocus || document.activeElement === document.body) view.focus()
        }
      }
    })()
    onPendingInput?.(task)
  })

  useLayoutEffect(() => {
    if (!parent.current) return
    const initial = initialState()
    const view = new EditorView({
      parent: parent.current,
      state: EditorState.create({
        doc: initial.documentValue,
        extensions: [
          placeholder(writingHint),
          history(),
          keymap.of([{ key: 'Enter', run: completeFence }, ...defaultKeymap, ...historyKeymap]),
          markdown({ addKeymap: false }),
          liveMarkdown,
          EditorView.lineWrapping,
          editability.of(editableState(!initial.disabled)),
          EditorView.contentAttributes.of({ 'aria-label': label, role: 'textbox', 'aria-multiline': 'true' }),
          EditorView.updateListener.of(update => {
            if (update.docChanged && !update.transactions.some(transaction => transaction.annotation(externalValue))) {
              changed(update.state.doc.toString())
            }
          }),
          EditorView.domEventHandlers({
            paste(event, view) {
              const item = Array.from(event.clipboardData?.items ?? []).find(item => item.kind === 'file' && item.type.startsWith('image/'))
              const file = item?.getAsFile() ?? Array.from(event.clipboardData?.files ?? []).find(file => file.type.startsWith('image/'))
              if (!file) return false
              event.preventDefault()
              pasteImage(view, file)
              return true
            },
          }),
        ],
      }),
    })
    editor.current = view
    return () => { editor.current = null; locked.current = false; view.destroy() }
  }, [documentId, label, editability])

  useLayoutEffect(() => {
    const view = editor.current
    if (view && view.state.doc.toString() !== documentValue) {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: documentValue },
        annotations: [externalValue.of(true), Transaction.addToHistory.of(false)],
      })
    }
  }, [documentValue])
  useLayoutEffect(() => {
    editor.current?.dispatch({ effects: editability.reconfigure(editableState(!disabled && !locked.current)) })
  }, [disabled, editability])

  return <div className="markdown-editor" aria-busy={importing}>
    {imageError && <div className="writing-image-error" role="alert">{imageError}</div>}
    {importing && <div className="sr-only" role="status">正在保存图片</div>}
    <div className="writing-input-host" ref={parent}/>
  </div>
}
