import { useLayoutEffect, useRef } from 'react'

export default function CollectionShell({ wide, detailId, list, detail }: {
  wide: boolean; detailId?: string; list: React.ReactNode; detail: React.ReactNode
}) {
  const detailPane = useRef<HTMLDivElement>(null)
  const listPane = useRef<HTMLDivElement>(null)
  const previous = useRef(detailId)
  useLayoutEffect(() => {
    const last = previous.current
    previous.current = detailId
    const pane = detailPane.current
    if (pane && detailId) {
      if (last !== detailId) pane.scrollTop = 0
      if (!wide && !pane.contains(document.activeElement)) pane.focus({ preventScroll: true })
    } else if (!wide && last) {
      const rows = listPane.current?.querySelectorAll<HTMLElement>('[data-record-id]')
      const row = Array.from(rows ?? []).find(element => element.dataset.recordId === last)
      if (row) row.focus()
      else listPane.current?.querySelector<HTMLElement>('input')?.focus()
    }
  }, [detailId, wide])

  return <div className={`collection-shell ${wide ? '' : 'collection-compact'}`}>
    <div className="collection-list-pane" hidden={!wide && !!detailId} ref={listPane}>{list}</div>
    <div className="collection-detail-pane" hidden={!wide && !detailId} ref={detailPane} tabIndex={-1}>{detail}</div>
  </div>
}
