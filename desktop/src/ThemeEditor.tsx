import useUnsavedChanges from './useUnsavedChanges'
import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { Alert, Button, InputNumber, Select, Slider, Space, Switch } from 'antd'

import { apply, colors, hex, presets, readTheme, type Theme, type ThemeChange } from './theme'

function sameColors(a: Theme, b: Theme) { return colors.every(([key]) => a[key].join(',') === b[key].join(',')) }

export default function ThemeEditor({ setWorkspaceDirty }: { setWorkspaceDirty?: (dirty: boolean) => void }) {
  const savedRef = useRef<Theme | null>(null)
  const [theme, setTheme] = useState<Theme | null>(null), [saved, setSaved] = useState<Theme | null>(null), [error, setError] = useState(''), [notice, setNotice] = useState('')
  useUnsavedChanges(JSON.stringify(theme) !== JSON.stringify(saved), setWorkspaceDirty)
  useEffect(() => () => { if (savedRef.current) apply(savedRef.current) }, [])
  useEffect(() => {
    let cancelled = false
    void readTheme().then(t => { if (!cancelled) { setTheme(t); setSaved(t); savedRef.current = t; apply(t) } }).catch(e => { if (!cancelled) setError(`读取主题失败：${String(e)}`) })
    return () => { cancelled = true }
  }, [])
  useEffect(() => {
    const sync = (event: Event) => {
      const { theme: next, persisted } = (event as CustomEvent<ThemeChange>).detail
      if (!persisted) return
      setTheme(next)
      setSaved(next)
      savedRef.current = next
      setNotice('')
    }
    window.addEventListener('personaldesk-theme', sync)
    return () => window.removeEventListener('personaldesk-theme', sync)
  }, [])
  function update(next: Theme) { setTheme(next); apply(next); setNotice('') }
  function applyPreset(preset: Theme) { if (!theme) return; update({ ...theme, ...preset, background: theme.background, zoom: theme.zoom, animations: theme.animations, surface_opacity: theme.surface_opacity, sidebar_opacity: theme.sidebar_opacity, image_opacity: theme.image_opacity, content_overlay: preset.content_overlay }) }
  async function save() { if (!theme) return; try { const persisted = await invoke<Theme>('theme_save', { theme }); setTheme(persisted); setSaved(persisted); savedRef.current = persisted; apply(persisted, 'persisted'); setError(''); setNotice('主题已保存') } catch (e) { setError(`保存主题失败：${String(e)}`) } }
  async function pick() { try { const path = await invoke<string | null>('theme_pick_image'); if (path && theme) update({ ...theme, background: path }) } catch (e) { setError(`选择背景失败：${String(e)}`) } }
  if (!theme) return error ? <Alert type="error" message={error}/> : null
  const activePreset = presets.find(preset => sameColors(theme, preset.theme))?.name
  return <div className="theme-editor">{error && <Alert type="error" message={error}/>} {notice && <Alert type="success" message={notice}/>}
    <div className="theme-presets" aria-label="主题预设">{presets.map(preset => <button type="button" key={preset.name} className={`theme-preset ${activePreset === preset.name ? 'active' : ''}`} aria-pressed={activePreset === preset.name} onClick={() => applyPreset(preset.theme)}><span className="theme-swatches" aria-hidden="true"><i style={{ background: hex(preset.theme.sidebar) }}/><i style={{ background: hex(preset.theme.canvas) }}/><i style={{ background: hex(preset.theme.accent) }}/><i style={{ background: hex(preset.theme.surface) }}/></span><strong>{preset.name}</strong><small>{preset.hint}</small></button>)}</div>
    <div className="theme-color-grid">{colors.map(([key, label]) => <label className="theme-color-field" key={key}><span>{label}</span><input aria-label={label} type="color" value={hex(theme[key])} onChange={e => { const color = e.target.value; update({ ...theme, [key]: [1, 3, 5].map(i => parseInt(color.slice(i, i + 2), 16)) }) }}/></label>)}</div>
    <Space wrap style={{ marginTop: 16 }}><label>界面缩放 <InputNumber aria-label="界面缩放" min={0.8} max={1.5} step={0.05} value={theme.zoom} onChange={v => { if (v !== null) update({ ...theme, zoom: v }) }}/></label><label>启用动画 <Switch aria-label="启用动画" checked={theme.animations} onChange={animations => update({ ...theme, animations })}/></label></Space>
    <div style={{ marginTop: 16 }}><Space wrap><Button onClick={() => void pick()}>选择背景图片</Button><Button onClick={() => update({ ...theme, background: null })}>移除背景</Button><span className="meta">{theme.background || '未设置背景'}</span><Select aria-label="背景显示方式" value={theme.fit} onChange={fit => update({ ...theme, fit })} options={[{ value: 'Cover', label: '铺满裁切' }, { value: 'Contain', label: '完整显示' }, { value: 'Stretch', label: '拉伸' }]}/></Space></div>
    <div className="theme-sliders">{([['surface_opacity', '面板不透明度'], ['sidebar_opacity', '侧栏不透明度'], ['image_opacity', '背景图片不透明度'], ['content_overlay', '内容遮罩']] as const).map(([key, label]) => <label key={key}>{label}<Slider aria-label={label} min={0} max={1} step={0.01} value={theme[key]} onChange={value => update({ ...theme, [key]: value })}/></label>)}</div>
    <div style={{ marginTop: 16 }}><Space><Button type="primary" onClick={() => void save()}>保存主题</Button><Button onClick={() => { if (saved) { setTheme(saved); apply(saved); setNotice('已撤销未保存的修改') } }}>撤销修改</Button></Space></div>
  </div>
}
