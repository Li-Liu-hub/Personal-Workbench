export default function EmptyState({ icon, title, description, actions }: {
  icon?: React.ReactNode
  title: React.ReactNode
  description?: React.ReactNode
  actions?: React.ReactNode
}) {
  return <div className="empty-state">
    {icon && <div className="empty-state-icon">{icon}</div>}
    <strong>{title}</strong>
    {description && <p>{description}</p>}
    {actions && <div className="empty-state-actions">{actions}</div>}
  </div>
}