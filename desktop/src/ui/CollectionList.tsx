import { PushpinFilled, RightOutlined } from '@ant-design/icons'

export function CollectionFilters({ tags, selected, onChange }: {
  tags: string[]; selected: string; onChange: (value: string) => void
}) {
  const filters = [
    { key: 'all', label: '全部' },
    { key: 'untagged', label: '未分类' },
    ...tags.map(tag => ({ key: tag, label: tag })),
  ]
  return <div className="collection-filters" role="group" aria-label="按分类筛选">
    {filters.map(({ key, label }, index) => <button key={index < 2 ? key : 'tag:' + key} type="button" className="filter-chip" aria-pressed={selected === key} onClick={() => onChange(key)}>{label}</button>)}
  </div>
}

export function CollectionRow({ id, title, subtitle, current, pinned, onSelect }: {
  id: string; title: string; subtitle: string; current: boolean; pinned?: boolean; onSelect: () => void
}) {
  return <div role="listitem">
    <button type="button" className={`collection-row ${current ? 'active' : ''}`} data-record-id={id}
      aria-current={current ? 'true' : undefined} onClick={onSelect}>
      <span className="collection-row-mark" aria-hidden="true">{Array.from(title || '?')[0]}</span>
      <span className="collection-row-copy">
        <span className="collection-row-name" title={title}>{title}</span>
        <span className="collection-row-meta" title={subtitle}>{subtitle}</span>
      </span>
      {pinned && <span className="collection-row-pin" title="已置顶" aria-label="已置顶"><PushpinFilled/></span>}
      <RightOutlined className="collection-row-arrow" aria-hidden="true"/>
    </button>
  </div>
}
