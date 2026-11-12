import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'

export type WritingDocument = {
  id: string; book_id: string; title: string; body: string; tags: string[]; revision: number; updated_at: string
}
export type RegisterWritingGuard = (saveBeforeLeaving: () => Promise<boolean>) => () => void
export type SaveStatus = 'saved' | 'pending' | 'saving' | 'error'

type Buffer = {
  page: WritingDocument
  version: number
  saved: number
  drafted: number
  queue: Promise<void>
  saving: Promise<boolean> | null
  draftTimer: number | null
  saveTimer: number | null
  inputs: Set<Promise<boolean>>
}

function cancelTimers(buffer: Buffer) {
  if (buffer.draftTimer !== null) window.clearTimeout(buffer.draftTimer)
  if (buffer.saveTimer !== null) window.clearTimeout(buffer.saveTimer)
  buffer.draftTimer = buffer.saveTimer = null
}

function enqueue<T>(buffer: Buffer, run: () => Promise<T>) {
  const task = buffer.queue.then(run)
  buffer.queue = task.then(() => {}, () => {})
  return task
}

/** Each open document owns its queue and edit counter; late saves cannot replace newer input. */
export default function useWritingAutosave({
  setDirty, registerGuard, onSaved, untitled = '未命名文档',
}: {
  setDirty: (dirty: boolean) => void
  registerGuard?: RegisterWritingGuard
  onSaved?: (page: WritingDocument) => void
  untitled?: string
}) {
  const current = useRef<Buffer | null>(null)
  const mounted = useRef(true)
  const [page, setPage] = useState<WritingDocument | null>(null)
  const [status, setStatus] = useState<SaveStatus>('saved')
  const [error, setError] = useState('')

  const forStorage = useCallback((document: WritingDocument) =>
    document.title.trim() ? document : { ...document, title: untitled }, [untitled])
  const checkpoint = useCallback(async (buffer: Buffer, document: WritingDocument, version: number) => {
    if (buffer.drafted === version) return
    await invoke('writing_draft', { page: forStorage(document) })
    buffer.drafted = version
  }, [forStorage])

  const commit = useCallback((buffer: Buffer) => {
    if (buffer.saving) return buffer.saving
    const task = enqueue(buffer, async () => {
      if (buffer.saved === buffer.version) return true
      const { page: submitted, version } = buffer
      if (current.current === buffer) setStatus('saving')
      try {
        // The backend records the draft before committing this revision.
        const saved = await invoke<WritingDocument>('writing_save', { page: forStorage(submitted) })
        buffer.saved = version
        const latest = buffer.version === version
        buffer.page = {
          ...buffer.page,
          revision: saved.revision,
          updated_at: saved.updated_at,
        }
        if (current.current === buffer) {
          setPage(buffer.page)
          setDirty(!latest)
          setStatus(latest ? 'saved' : 'pending')
          setError('')
          onSaved?.(saved)
        }
        return true
      } catch (cause) {
        if (current.current === buffer) {
          setStatus('error')
          setError('自动保存失败：' + String(cause))
        }
        return false
      }
    })
    buffer.saving = task
    void task.then(() => { if (buffer.saving === task) buffer.saving = null })
    return task
  }, [forStorage, onSaved, setDirty])

  const flush = useCallback(async () => {
    const buffer = current.current
    if (!buffer) return true
    while (current.current === buffer) {
      cancelTimers(buffer)
      if (buffer.inputs.size) {
        const completed = await Promise.all([...buffer.inputs].map(task => task.catch(() => false)))
        if (completed.some(ok => !ok)) return false
      } else {
        if (buffer.saved === buffer.version) return true
        if (!await commit(buffer)) return false
      }
    }
    return true
  }, [commit])

  const load = useCallback((document: WritingDocument | null) => {
    if (!mounted.current) return
    if (current.current) cancelTimers(current.current)
    current.current = document ? {
      page: document, version: 0, saved: 0, drafted: 0, queue: Promise.resolve(),
      saving: null, draftTimer: null, saveTimer: null, inputs: new Set(),
    } : null
    setPage(document)
    setDirty(false)
    setStatus('saved')
    setError('')
  }, [setDirty])

  const edit = useCallback((patch: Partial<Pick<WritingDocument, 'title' | 'body'>>) => {
    const buffer = current.current
    if (!buffer || (Object.keys(patch) as (keyof typeof patch)[]).every(key => buffer.page[key] === patch[key])) return
    buffer.page = { ...buffer.page, ...patch }
    buffer.version += 1
    setPage(buffer.page)
    setDirty(true)
    setStatus('pending')
    setError('')
    // Checkpoint during continuous typing too; commit after a short pause.
    if (buffer.draftTimer === null) buffer.draftTimer = window.setTimeout(() => {
      buffer.draftTimer = null
      void enqueue(buffer, async () => {
        if (current.current !== buffer || buffer.version === buffer.saved) return
        try { await checkpoint(buffer, buffer.page, buffer.version) } catch (cause) {
          if (current.current === buffer) {
            setStatus('error')
            setError('暂时无法保存草稿：' + String(cause))
          }
        }
      })
    }, 240)
    if (buffer.saveTimer !== null) window.clearTimeout(buffer.saveTimer)
    buffer.saveTimer = window.setTimeout(() => { void flush() }, 900)
  }, [checkpoint, flush, setDirty])

  const trackInput = useCallback((task: Promise<boolean>) => {
    const buffer = current.current
    if (!buffer) return
    buffer.inputs.add(task)
    void task.then(() => buffer.inputs.delete(task), () => buffer.inputs.delete(task))
  }, [])

  const snapshot = useCallback(() => current.current ? forStorage(current.current.page) : null, [forStorage])
  useLayoutEffect(() => registerGuard?.(flush), [registerGuard, flush])
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!event.isComposing && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's' && current.current) {
        event.preventDefault()
        void flush()
      }
    }
    const beforeUnload = (event: BeforeUnloadEvent) => {
      const buffer = current.current
      if (buffer && (buffer.saved !== buffer.version || buffer.inputs.size)) { event.preventDefault(); event.returnValue = '' }
    }
    window.addEventListener('keydown', onKey)
    window.addEventListener('beforeunload', beforeUnload)
    return () => {
      window.removeEventListener('keydown', onKey)
      window.removeEventListener('beforeunload', beforeUnload)
    }
  }, [flush])
  useLayoutEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      if (current.current) cancelTimers(current.current)
      current.current = null
    }
  }, [])
  return { page, status, error, load, edit, flush, trackInput, snapshot }
}
