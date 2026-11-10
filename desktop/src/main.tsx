import { StrictMode, useEffect, useState } from 'react'
import {invoke} from '@tauri-apps/api/core'
import {apply, type Theme} from './ThemeEditor'
import { createRoot } from 'react-dom/client'
import { ConfigProvider } from 'antd'
import zhCN from 'antd/locale/zh_CN'
import './index.css'
import App from './App.tsx'

function Root(){
 const [theme,setTheme]=useState<Theme|null>(null)
 useEffect(()=>{const update=(event:Event)=>{const t=(event as CustomEvent<Theme>).detail;setTheme(t)};window.addEventListener('personaldesk-theme',update);void invoke<Theme>('theme_get').then(apply).catch(()=>{});return()=>window.removeEventListener('personaldesk-theme',update)},[])
 const color=(value:number[]|undefined,fallback:string)=>value?'#'+value.map(n=>n.toString(16).padStart(2,'0')).join(''):fallback
 return <ConfigProvider locale={zhCN} button={{autoInsertSpace:false}} theme={{token:{colorPrimary:color(theme?.accent,'#3a6559'),colorText:color(theme?.text,'#28332f'),colorTextSecondary:color(theme?.muted,'#7b8580'),colorBorder:color(theme?.border,'#e3e7e4'),colorBgContainer:color(theme?.surface,'#fff'),colorFillAlter:color(theme?.input,'#f7f8f7'),borderRadius:6,controlHeight:32,fontSize:13,fontFamily:'Segoe UI, Microsoft YaHei UI, sans-serif'},components:{Button:{primaryShadow:'none',defaultShadow:'none'},Table:{headerBg:color(theme?.sidebar,'#f4f6f4')},Tree:{nodeSelectedBg:'color-mix(in srgb, var(--desk-accent) 10%, transparent)',titleHeight:30}}}}><App/></ConfigProvider>
}
createRoot(document.getElementById('root')!).render(<StrictMode><Root/></StrictMode>)
