import { CheckOutlined, ClockCircleOutlined, WarningOutlined } from '@ant-design/icons'
import type { SaveStatus } from './useWritingAutosave'

export default function WritingSaveStatus({ status }: { status: SaveStatus }) {
  const label = { saved: '已保存', pending: '等待自动保存', saving: '正在保存…', error: '保存失败' }[status]
  return <span className="save-state" data-state={status} role="status">
    {status === 'saved' ? <CheckOutlined aria-hidden="true"/> : status === 'error' ? <WarningOutlined aria-hidden="true"/> : <ClockCircleOutlined aria-hidden="true"/>}
    {label}
  </span>
}
