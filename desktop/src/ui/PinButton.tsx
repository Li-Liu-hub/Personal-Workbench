import { PushpinFilled, PushpinOutlined } from '@ant-design/icons'

export default function PinButton({ pinned, onClick, label, disabled = false }: { pinned: boolean; onClick: () => void; label?: string; disabled?: boolean }) {
  const text = pinned ? '取消置顶' : '置顶'
  return <button
    type="button"
    className={`icon-button ${pinned ? 'on' : ''}`}
    aria-label={label ? `${text}${label}` : text}
    title={text}
    disabled={disabled}
    onClick={onClick}
  >{pinned ? <PushpinFilled/> : <PushpinOutlined/>}</button>
}