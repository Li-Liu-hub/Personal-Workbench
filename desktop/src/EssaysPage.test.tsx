// @vitest-environment jsdom
import { EditorView } from '@codemirror/view'
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react'
import EssaysPage from './EssaysPage'
import type { WritingDocument } from './useWritingAutosave'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); invoke.mockReset() })

it('返回今日主题前保存刚输入的正文，无需按保存键', async () => {
  const topic = { day: '2026-11-05', topic: { id: 'x', category: '生活', title: '今日主题', prompt: '提示' }, page_id: null }
  const page = { id: '1', book_id: 'root', title: '标题', body: '', tags: [], revision: 1, updated_at: '' }
  invoke.mockImplementation(async (name: string, args?: { page: WritingDocument }) => {
    if (name === 'essay_today') return topic
    if (name === 'essay_start') return page
    if (name === 'writing_save') return { ...args?.page, revision: 2 }
  })
  render(<EssaysPage setDirty={() => {}}/>)
  fireEvent.click(await screen.findByRole('button', { name: '开始写作' }))
  const editor = EditorView.findFromDOM(await screen.findByRole('textbox', { name: '正文' }))!
  act(() => editor.dispatch({ changes: { from: 0, insert: '刚输入的随笔' } }))
  fireEvent.click(screen.getByRole('button', { name: '返回' }))
  await screen.findByRole('button', { name: '开始写作' })
  expect(invoke).toHaveBeenCalledWith('writing_save', { page: { ...page, body: '刚输入的随笔' } })
})
