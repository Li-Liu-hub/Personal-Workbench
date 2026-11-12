// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import useWritingAutosave, { type WritingDocument } from './useWritingAutosave'

const invoke = vi.hoisted(() => vi.fn())
vi.mock('@tauri-apps/api/core', () => ({ invoke }))
afterEach(() => { cleanup(); vi.useRealTimers(); invoke.mockReset() })

const document: WritingDocument = { id: 'page', book_id: 'book', title: '记录', body: '', tags: ['保留的标签'], revision: 1, updated_at: '' }

it('停笔自动保存，慢保存返回时不会覆盖其间输入的内容', async () => {
  vi.useFakeTimers()
  let completeFirst: () => void = () => {}
  const submissions: WritingDocument[] = []
  invoke.mockImplementation((name: string, { page }: { page: WritingDocument }) => {
    if (name === 'writing_draft') return Promise.resolve()
    submissions.push(page)
    const saved = { ...page, revision: page.revision + 1 }
    if (submissions.length === 1) return new Promise(resolve => { completeFirst = () => resolve(saved) })
    return Promise.resolve(saved)
  })
  const setDirty = vi.fn()
  const { result } = renderHook(() => useWritingAutosave({ setDirty }))
  act(() => { result.current.load(document); result.current.edit({ body: '第一笔' }) })
  await act(() => vi.advanceTimersByTimeAsync(300))
  expect(invoke).toHaveBeenCalledWith('writing_draft', { page: { ...document, body: '第一笔' } })
  await act(() => vi.advanceTimersByTimeAsync(650))
  expect(submissions).toHaveLength(1)
  act(() => result.current.edit({ body: '第一笔和第二笔' }))
  expect(result.current.page?.body).toBe('第一笔和第二笔')
  await act(async () => {
    const leaving = result.current.flush()
    completeFirst()
    expect(await leaving).toBe(true)
  })
  expect(submissions).toEqual([
    { ...document, body: '第一笔' },
    { ...document, revision: 2, body: '第一笔和第二笔' },
  ])
  expect(result.current.page).toMatchObject({ body: '第一笔和第二笔', revision: 3 })
  expect(result.current.status).toBe('saved')
  expect(setDirty).toHaveBeenLastCalledWith(false)
})

it('立即离开时等待图片写入，标题暂空也能保存正文', async () => {
  invoke.mockImplementation(async (_name: string, { page }: { page: WritingDocument }) => ({ ...page, revision: 2 }))
  const setDirty = vi.fn()
  const { result } = renderHook(() => useWritingAutosave({ setDirty }))
  let completeImage: (ok: boolean) => void = () => {}
  const image = new Promise<boolean>(resolve => { completeImage = resolve })
  act(() => {
    result.current.load(document)
    result.current.edit({ title: '' })
    result.current.trackInput(image)
  })
  const leaving = result.current.flush()
  expect(invoke).not.toHaveBeenCalled()
  await act(async () => {
    result.current.edit({ body: '![图片](data/picture/example.png)' })
    completeImage(true)
    expect(await leaving).toBe(true)
  })
  expect(invoke).toHaveBeenCalledWith('writing_save', { page: {
    ...document, title: '未命名文档', body: '![图片](data/picture/example.png)',
  } })
  expect(result.current.page?.title).toBe('')
})
