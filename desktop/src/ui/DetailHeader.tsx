import { ArrowLeftOutlined } from '@ant-design/icons'

export default function DetailHeader({ title, subtitle, actions, onBack, backLabel = '返回列表' }: {
  title: React.ReactNode
  subtitle?: React.ReactNode
  actions?: React.ReactNode
  onBack?: () => void
  backLabel?: string
}) {
  return <header className="detail-header">
    {onBack && <button type="button" className="detail-back" onClick={onBack}><ArrowLeftOutlined aria-hidden="true"/>{backLabel}</button>}
    <div className="detail-heading">
      <h1 className="detail-title">{title}</h1>
      {subtitle && <div className="detail-subtitle">{subtitle}</div>}
    </div>
    {actions && <div className="detail-actions">{actions}</div>}
  </header>
}
