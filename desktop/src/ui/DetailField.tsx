export default function DetailField({ label, value, mono = false, actions, placeholder = '—', empty }: {
  label: string
  value?: React.ReactNode
  mono?: boolean
  actions?: React.ReactNode
  placeholder?: string
  empty?: boolean
}) {
  const isEmpty = empty ?? (value === undefined || value === null || value === '')
  return <div className="field-row">
    <span className="field-label">{label}</span>
    <div className={`field-value ${mono ? 'mono' : ''} ${isEmpty ? 'placeholder' : ''}`}>{isEmpty ? placeholder : value}</div>
    {actions && <div className="field-actions">{actions}</div>}
  </div>
}