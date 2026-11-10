// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import useAutoSave from './useAutoSave'
afterEach(() => vi.useRealTimers())
it('连续输入只保存最后版本，失败不循环重试，修改后再保存', async () => {
  vi.useFakeTimers()
  const save = vi.fn().mockResolvedValue(undefined)
  const hook = renderHook(({ value, busy }) => useAutoSave(value, true, busy, save), { initialProps: { value: { body: '旧内容' }, busy: false } })
  await act(() => vi.advanceTimersByTimeAsync(1000))
  hook.rerender({ value: { body: '新内容' }, busy: false })
  await act(() => vi.advanceTimersByTimeAsync(1499))
  expect(save).not.toHaveBeenCalled()
  await act(() => vi.advanceTimersByTimeAsync(1))
  expect(save).toHaveBeenCalledTimes(1)
  // A failed attempt leaves dirty true and changes busy true -> false.
  hook.rerender({ value: { body: '新内容' }, busy: true })
  hook.rerender({ value: { body: '新内容' }, busy: false })
  await act(() => vi.advanceTimersByTimeAsync(10000))
  expect(save).toHaveBeenCalledTimes(1)
  hook.rerender({ value: { body: '再次修改' }, busy: false })
  await act(() => vi.advanceTimersByTimeAsync(1500))
  expect(save).toHaveBeenCalledTimes(2)
  hook.unmount()
})
