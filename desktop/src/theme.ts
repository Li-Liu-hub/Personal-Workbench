import { invoke } from '@tauri-apps/api/core'

export type Theme = { accent: number[]; text: number[]; muted: number[]; canvas: number[]; sidebar: number[]; surface: number[]; input: number[]; border: number[]; button_text: number[]; sidebar_text: number[]; surface_opacity: number; sidebar_opacity: number; image_opacity: number; content_overlay: number; zoom: number; animations: boolean; background: string | null; fit: 'Cover' | 'Contain' | 'Stretch' }
type ColorKey = 'accent' | 'text' | 'muted' | 'canvas' | 'sidebar' | 'surface' | 'input' | 'border' | 'button_text' | 'sidebar_text'
export const colors: [ColorKey, string][] = [['accent', '主题色'], ['text', '正文文字'], ['muted', '次要文字'], ['canvas', '页面背景'], ['sidebar', '侧栏背景'], ['surface', '卡片与面板'], ['input', '输入框'], ['border', '边框'], ['button_text', '按钮文字'], ['sidebar_text', '侧栏文字']]
export const hex = (v: number[]) => '#' + v.map(n => n.toString(16).padStart(2, '0')).join('')

export const isDarkTheme = (theme: Theme) => theme.canvas[0] * 0.2126 + theme.canvas[1] * 0.7152 + theme.canvas[2] * 0.0722 < 96

export const presets: { name: string; hint: string; theme: Theme }[] = [
  { name: '素纸', hint: '墨绿桌面，纸白内容', theme: { accent: [43, 86, 70], text: [33, 38, 31], muted: [100, 108, 100], canvas: [246, 243, 235], sidebar: [23, 38, 31], surface: [255, 254, 250], input: [241, 237, 227], border: [219, 213, 198], button_text: [255, 254, 250], sidebar_text: [201, 211, 202], surface_opacity: 1, sidebar_opacity: 1, image_opacity: .7, content_overlay: .12, zoom: 1, animations: true, background: null, fit: 'Cover' } },
  { name: '夜读', hint: '深色桌面，低亮度阅读', theme: { accent: [143, 184, 165], text: [232, 228, 216], muted: [154, 166, 156], canvas: [29, 38, 33], sidebar: [20, 28, 24], surface: [35, 46, 39], input: [42, 54, 46], border: [52, 66, 58], button_text: [20, 28, 24], sidebar_text: [190, 202, 192], surface_opacity: 1, sidebar_opacity: 1, image_opacity: .55, content_overlay: .18, zoom: 1, animations: true, background: null, fit: 'Cover' } },
  { name: '青瓷', hint: '冷色纸面，清透安静', theme: { accent: [58, 92, 96], text: [32, 42, 44], muted: [102, 116, 118], canvas: [242, 246, 245], sidebar: [29, 45, 47], surface: [252, 255, 254], input: [231, 240, 238], border: [214, 226, 223], button_text: [252, 255, 254], sidebar_text: [190, 207, 204], surface_opacity: 1, sidebar_opacity: 1, image_opacity: .7, content_overlay: .12, zoom: 1, animations: true, background: null, fit: 'Cover' } },
  { name: '夜航', hint: '深夜蓝调，专注沉浸', theme: { accent: [125, 165, 230], text: [225, 230, 240], muted: [148, 158, 175], canvas: [24, 28, 38], sidebar: [17, 20, 28], surface: [31, 36, 48], input: [38, 44, 58], border: [52, 60, 76], button_text: [17, 20, 28], sidebar_text: [185, 195, 212], surface_opacity: 1, sidebar_opacity: 1, image_opacity: .55, content_overlay: .18, zoom: 1, animations: true, background: null, fit: 'Cover' } },
]

export type ThemeChange = { theme: Theme; persisted: boolean }

let previewSequence = 0
let pendingTheme: Promise<Theme> | undefined
let background: { path: string; request: Promise<string> } | undefined

export function readTheme() {
  return pendingTheme ??= invoke<Theme>('theme_get').finally(() => { pendingTheme = undefined })
}

function readBackground(path: string) {
  // Imported background names are unique. Preview an external path afresh.
  if (!path.startsWith('theme-assets/')) return invoke<string>('theme_image', { value: path })
  if (background?.path === path) return background.request
  const request = invoke<string>('theme_image', { value: path }).catch(error => {
    if (background?.request === request) background = undefined
    throw error
  })
  background = { path, request }
  return request
}

export function apply(theme: Theme, origin: 'editor' | 'persisted' = 'editor') {
  const root = document.documentElement
  for (const [key] of colors) root.style.setProperty(`--desk-${key.replaceAll('_', '-')}`, hex(theme[key]))
  const rgba = (rgb: number[], alpha: number) => `rgba(${rgb.join(',')},${alpha})`
  root.style.setProperty('--desk-surface-alpha', rgba(theme.surface, theme.surface_opacity))
  root.style.setProperty('--desk-sidebar-alpha', rgba(theme.sidebar, theme.sidebar_opacity))
  root.style.setProperty('--desk-overlay', rgba(theme.canvas, theme.background ? theme.content_overlay : 1))
  root.style.setProperty('--desk-image-opacity', String(theme.image_opacity))
  root.style.setProperty('--desk-image-fit', theme.fit === 'Stretch' ? '100% 100%' : theme.fit.toLowerCase())
  root.style.setProperty('--desk-zoom', String(theme.zoom))
  root.style.setProperty('--desk-motion', theme.animations ? '0.16s' : '0s')
  const dark = isDarkTheme(theme)
  root.style.setProperty('--desk-seal', dark ? '#d0705a' : '#b4472f')
  root.style.setProperty('--desk-success', dark ? '#82c9a0' : '#2e7d54')
  root.style.setProperty('--desk-warning', dark ? '#d9a75f' : '#a06b1f')
  root.style.setProperty('--desk-info', dark ? '#8ab4e8' : '#33628f')
  const sequence = ++previewSequence
  if (theme.background) {
    void readBackground(theme.background).then(url => {
      if (sequence === previewSequence) root.style.setProperty('--desk-background-image', `url("${url}")`)
    }).catch(() => {
      if (sequence === previewSequence) root.style.setProperty('--desk-background-image', 'none')
    })
  } else {
    background = undefined
    root.style.setProperty('--desk-background-image', 'none')
  }
  window.dispatchEvent(new CustomEvent<ThemeChange>('personaldesk-theme', { detail: { theme, persisted: origin === 'persisted' } }))
}
