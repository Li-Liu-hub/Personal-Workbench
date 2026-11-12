import { StrictMode, useEffect, useState } from 'react'
import { apply, isDarkTheme, readTheme, type Theme, type ThemeChange } from './theme'
import { createRoot } from 'react-dom/client'
import { ConfigProvider, theme as antdTheme } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import './styles/index.css'
import App from './App.tsx'

const rgb = (value: number[] | undefined, fallback: string) => value ? '#' + value.map(n => n.toString(16).padStart(2, '0')).join('') : fallback
const mix = (value: number[] | undefined, fallback: string, other: [number, number, number], amount: number) => {
  const base = value ?? [parseInt(fallback.slice(1, 3), 16), parseInt(fallback.slice(3, 5), 16), parseInt(fallback.slice(5, 7), 16)]
  return '#' + base.map((n, i) => Math.round(n + (other[i] - n) * amount).toString(16).padStart(2, '0')).join('')
}
const alpha = (value: number[] | undefined, fallback: string, a: number) => {
  const base = value ?? [parseInt(fallback.slice(1, 3), 16), parseInt(fallback.slice(3, 5), 16), parseInt(fallback.slice(5, 7), 16)]
  return `rgba(${base.join(',')},${a})`
}

function Root() {
  const [theme, setTheme] = useState<Theme | null>(null)
  useEffect(() => {
    let active = true
    const update = (event: Event) => setTheme((event as CustomEvent<ThemeChange>).detail.theme)
    window.addEventListener('personaldesk-theme', update)
    void readTheme().then(current => { if (active) apply(current, 'persisted') }).catch(() => {})
    return () => { active = false; window.removeEventListener('personaldesk-theme', update) }
  }, [])
  const dark = theme ? isDarkTheme(theme) : false
  const accent = theme?.accent
  return <ConfigProvider
    locale={zhCN}
    button={{ autoInsertSpace: false }}
    wave={{ disabled: true }}
    theme={{
      algorithm: dark ? antdTheme.darkAlgorithm : antdTheme.defaultAlgorithm,
      token: {
        colorPrimary: rgb(accent, '#2b5646'),
        colorPrimaryHover: mix(accent, '#2b5646', [255, 255, 255], 0.14),
        colorPrimaryActive: mix(accent, '#2b5646', [0, 0, 0], 0.14),
        colorText: rgb(theme?.text, '#21261f'),
        colorTextSecondary: rgb(theme?.muted, '#646c64'),
        colorTextLightSolid: rgb(theme?.button_text, '#fffefa'),
        colorBorder: rgb(theme?.border, '#dbd5c6'),
        colorBorderSecondary: rgb(theme?.border, '#dbd5c6'),
        colorBgContainer: rgb(theme?.surface, '#fffefa'),
        colorBgElevated: rgb(theme?.surface, '#fffefa'),
        colorBgLayout: rgb(theme?.canvas, '#f6f3eb'),
        colorFillAlter: rgb(theme?.input, '#f1ede3'),
        colorFillSecondary: rgb(theme?.input, '#f1ede3'),
        colorLink: rgb(accent, '#2b5646'),
        borderRadius: 6,
        borderRadiusLG: 10,
        borderRadiusSM: 4,
        controlHeight: 36,
        controlHeightSM: 28,
        controlHeightLG: 40,
        fontSize: 14,
        fontFamily: 'Microsoft YaHei UI, Segoe UI, sans-serif',
        boxShadow: '0 2px 8px rgba(23,38,31,0.10), 0 16px 48px -8px rgba(23,38,31,0.26)',
        boxShadowSecondary: '0 2px 8px rgba(23,38,31,0.08), 0 12px 32px -8px rgba(23,38,31,0.18)',
        motion: theme?.animations ?? true,
        motionDurationFast: '0.12s',
        motionDurationMid: '0.18s',
        motionDurationSlow: '0.28s',
        motionEaseInOut: 'cubic-bezier(0.65, 0, 0.35, 1)',
        motionEaseOut: 'cubic-bezier(0.22, 1, 0.36, 1)',
      },
      components: {
        Button: { primaryShadow: 'none', defaultShadow: 'none', dangerShadow: 'none', fontWeight: 500, paddingInline: 16 },
        Input: { activeShadow: `0 0 0 3px ${alpha(accent, '#2b5646', 0.14)}`, paddingInline: 12 },
        Table: {
          headerBg: rgb(theme?.input, '#f1ede3'),
          headerColor: rgb(theme?.muted, '#646c64'),
          borderColor: rgb(theme?.border, '#dbd5c6'),
          cellPaddingBlock: 11,
          cellPaddingInline: 12,
        },
        Tree: { nodeSelectedBg: 'color-mix(in srgb, var(--desk-accent) 10%, transparent)', titleHeight: 32, nodeHoverBg: 'color-mix(in srgb, var(--desk-input) 65%, transparent)' },
        Segmented: {
          trackBg: rgb(theme?.input, '#f1ede3'),
          itemSelectedBg: rgb(theme?.surface, '#fffefa'),
        },
        Modal: {
          contentBg: rgb(theme?.surface, '#fffefa'),
          headerBg: rgb(theme?.surface, '#fffefa'),
          footerBg: rgb(theme?.surface, '#fffefa'),
        },
        Drawer: { colorBgElevated: rgb(theme?.surface, '#fffefa') },
        Tabs: { itemSelectedColor: rgb(accent, '#2b5646'), inkBarColor: rgb(accent, '#2b5646') },
        Tooltip: { colorBgSpotlight: rgb(theme?.sidebar, '#17261f'), colorTextLightSolid: rgb(theme?.surface, '#fffefa') },
        Alert: { containerBorderRadius: 8 },
      },
    }}
  ><App dark={dark}/></ConfigProvider>
}
createRoot(document.getElementById('root')!).render(<StrictMode><Root/></StrictMode>)
