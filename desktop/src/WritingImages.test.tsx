// @vitest-environment jsdom
import { useState } from 'react'
import { EditorView } from '@codemirror/view'
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import MarkdownEditor, { normalizeLegacyImages } from './MarkdownEditor'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
  Object.defineProperty(Range.prototype, 'getClientRects', { value: () => [] })
  Object.defineProperty(Range.prototype, 'getBoundingClientRect', { value: () => ({ left: 0, right: 0, top: 0, bottom: 0, width: 0, height: 0 }) })
})
afterEach(() => { cleanup(); invoke.mockReset() })

const documentId = '2cd37b92-5f85-4a6c-8a16-9ca90f6f86ae'
const path = 'data/picture/' + documentId + '/pasted.png'
const src = 'data:image/png;base64,AAAA'

it('粘贴后直接显示图片，光标在下一行，正文仍保存为 Markdown', async () => {
  invoke.mockImplementation(async (name: string) => name === 'writing_image_paste' ? path : src)
  const onChange = vi.fn()
  function Example() {
    const [value, setValue] = useState('前后')
    return <MarkdownEditor documentId={documentId} label="正文" value={value} onChange={next => { onChange(next); setValue(next) }}/>
  }
  render(<Example/>)
  const input = screen.getByRole('textbox', { name: '正文' })
  const view = EditorView.findFromDOM(input)!
  act(() => view.dispatch({ selection: { anchor: 1 } }))
  const file = { size: 3, arrayBuffer: async () => new Uint8Array([1, 2, 3]).buffer }
  fireEvent.paste(input, { clipboardData: { items: [{ kind: 'file', type: 'image/png', getAsFile: () => file }] } })
  await waitFor(() => expect(onChange).toHaveBeenLastCalledWith('前\n![图片](' + path + ')\n后'))
  const image = await screen.findByRole('img', { name: '图片' })
  await waitFor(() => expect(image.getAttribute('src')).toBe(src))
  expect(invoke).toHaveBeenCalledWith('writing_image_paste', { documentId, bytes: [1, 2, 3] })
  act(() => view.dispatch({ changes: { from: view.state.selection.main.head, insert: '继续' } }))
  expect(onChange).toHaveBeenLastCalledWith('前\n![图片](' + path + ')\n继续后')
})

it('重新打开时直接渲染已保存的图片引用', async () => {
  invoke.mockResolvedValue(src)
  render(<MarkdownEditor documentId={documentId} label="正文" value={'![图片](' + path + ')'} onChange={() => {}}/>)
  const image = await screen.findByRole('img', { name: '图片' })
  await waitFor(() => expect(image.getAttribute('src')).toBe(src))
  expect(invoke).toHaveBeenCalledWith('writing_image_read', { reference: path })
})

it('旧版 Obsidian 图片链接直接在正文中显示', async () => {
  const reference = 'writing-assets/obsidian-de89daf10329b338/733aa4970b160f72/Pasted%20image%2020261016160000.png'
  const legacy = '问题汇总\n[图片: Pasted image 20261016160000.png]\n(file:///C:/Users/example/AppData/Local/PersonalDesk/' + reference + ')'
  expect(normalizeLegacyImages(legacy)).toBe('问题汇总\n![Pasted image 20261016160000.png](' + reference + ')')
  invoke.mockResolvedValue(src)
  render(<MarkdownEditor documentId={documentId} label="正文" value={legacy} onChange={() => {}}/>)
  const image = await screen.findByRole('img', { name: 'Pasted image 20261016160000.png' })
  await waitFor(() => expect(image.getAttribute('src')).toBe(src))
  expect(invoke).toHaveBeenCalledWith('writing_image_read', { reference })
})
