import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, Space, Table, Tabs } from 'antd'

export type Chat = { id: string; title: string; format: string; text: string; sheets: { name: string; rows: string[][] }[] }
function columnName(index: number): string {
  let name = ''
  for (let n = index + 1; n > 0; n = Math.floor((n - 1) / 26)) name = String.fromCharCode(65 + (n - 1) % 26) + name
  return name
}
export default function ChatPreview({ chat, onExport }: { chat: Chat; onExport: () => void }) {
  const [sheetIndex, setSheetIndex] = useState('0')
  const [error, setError] = useState('')
  const [copied, setCopied] = useState(false)
  const sheet = chat.sheets[Number(sheetIndex)]
  const width = sheet?.rows.reduce((max, row) => Math.max(max, row.length), 0) ?? 0
  async function copy() {
    try { await invoke('copy_value', { value: chat.text }); setCopied(true); setError('') }
    catch (e) { setError(`复制失败：${String(e)}`) }
  }
  return <>
    <Space style={{ marginBottom: 12 }}><Button onClick={() => void copy()}>{copied ? '已复制全部文字' : '复制全部文字'}</Button><Button onClick={onExport}>导出原件</Button></Space>
    {error && <Alert type="error" message={error} />}
    {chat.sheets.length > 0 ? <>
      <Tabs activeKey={sheetIndex} onChange={setSheetIndex} items={chat.sheets.map((s, index) => ({ key: String(index), label: s.name }))} />
      {sheet && <><p>{sheet.rows.length} 行 · {width} 列</p><Table key={sheetIndex} size="small" rowKey="index" pagination={{ pageSize: 50, showSizeChanger: false }} scroll={{ x: 'max-content', y: 360 }} dataSource={sheet.rows.map((cells, index) => ({ cells, index }))} columns={[
        { title: '行号', key: 'index', render: (_, row) => row.index + 1 },
        ...Array.from({ length: width }, (_, index) => ({ title: columnName(index), key: String(index), render: (_: unknown, row: { cells: string[] }) => <span style={{ whiteSpace: 'pre-wrap' }}>{row.cells[index] ?? ''}</span> })),
      ]} /></>}
    </> : <pre style={{ whiteSpace: 'pre-wrap', maxHeight: 400, overflow: 'auto' }}>{chat.text}</pre>}
  </>
}
