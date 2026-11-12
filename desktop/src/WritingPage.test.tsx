// @vitest-environment jsdom
import { afterEach, beforeAll, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import WritingPage from './WritingPage'
import type { WritingDocument } from './useWritingAutosave'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
beforeAll(() => {
  Object.defineProperty(window, 'matchMedia', { value: () => ({ matches: false, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent() { return false } }) })
  globalThis.ResizeObserver = class { observe() {} unobserve() {} disconnect() {} }
})
afterEach(() => { cleanup(); invoke.mockReset() })

const book = { id: 'b', parent_id: null, title: '图书甲', tags: [] }
const document: WritingDocument = { id: 'p', book_id: book.id, title: '测试文档', body: '包含关键词', tags: ['已有标签'], revision: 1, updated_at: '' }
const root = '__personal_writing__'

it('创建文档时使用选中的图书', async () => {
  invoke.mockImplementation(async (name: string) => name === 'writing_directory' ? { books: [book], pages: [] } : document)
  render(<WritingPage setDirty={() => {}}/>)
  fireEvent.click(await screen.findByText('图书甲'))
  fireEvent.click(screen.getByRole('button', { name: '新建文档' }))
  await screen.findByRole('textbox', { name: '文档标题' })
  expect(invoke).toHaveBeenCalledWith('writing_create', { title: '未命名文档', bookId: 'b' })
})

it('搜索正文时保留上级目录并显示匹配文档', async () => {
  const { body: _body, ...entry } = document
  invoke.mockImplementation(async (name: string) =>
    name === 'writing_directory' ? { books: [book], pages: [entry] } : [entry.id])
  render(<WritingPage setDirty={() => {}}/>)
  await screen.findByText('图书甲')
  fireEvent.change(screen.getByLabelText('搜索文档'), { target: { value: '关键词' } })
  expect(await screen.findByText('测试文档')).toBeTruthy()
  expect(screen.getByText('图书甲')).toBeTruthy()
  expect(invoke).toHaveBeenCalledWith('writing_matches', { query: '关键词' })
  expect(invoke).not.toHaveBeenCalledWith('writing_list')
})

it('保存失败时留在原文档，重试后才打开目标文档', async () => {
  const first = { ...document, book_id: root, title: '第一篇' }
  const second = { ...first, id: 'q', title: '第二篇' }
  const saved = { ...first, title: '刚输入的标题' }
  let attempts = 0
  invoke.mockImplementation(async (name: string, args?: { page?: WritingDocument; id?: string }) => {
    if (name === 'writing_directory') return { pages: [first, second], books: [] }
    if (name === 'writing_page') return args?.id === first.id ? first : second
    if (name === 'writing_save') {
      if (++attempts === 1) throw new Error('磁盘暂时不可写')
      return { ...args?.page, revision: 2 }
    }
  })
  render(<WritingPage setDirty={() => {}}/>)
  fireEvent.click(await screen.findByText('第一篇'))
  const title = await screen.findByRole<HTMLInputElement>('textbox', { name: '文档标题' })
  fireEvent.change(title, { target: { value: saved.title } })
  fireEvent.click(screen.getByText('第二篇'))
  await screen.findByText(/磁盘暂时不可写/)
  expect(title.value).toBe(saved.title)
  fireEvent.click(screen.getByText('第二篇'))
  await waitFor(() => expect(title.value).toBe(second.title))
  expect(invoke.mock.calls.filter(([name]) => name === 'writing_save')).toEqual([
    ['writing_save', { page: saved }], ['writing_save', { page: saved }],
  ])
})
