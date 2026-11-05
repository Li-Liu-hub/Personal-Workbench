import {useEffect,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {Button,Popover,Space} from 'antd'
import {apply,type Theme} from './ThemeEditor'
export default function WritingZoom(){
 const [zoom,setZoom]=useState(1),[error,setError]=useState(''),[busy,setBusy]=useState(false)
 useEffect(()=>{let alive=true;void invoke<Theme>('theme_get').then(t=>{if(alive)setZoom(t.zoom)}).catch(e=>{if(alive)setError(String(e))});const changed=(event:Event)=>setZoom((event as CustomEvent<Theme>).detail.zoom);window.addEventListener('personaldesk-theme',changed);return()=>{alive=false;window.removeEventListener('personaldesk-theme',changed)}},[])
 async function set(value:number){if(busy)return;setBusy(true);try{const theme=await invoke<Theme>('theme_set_zoom',{zoom:Math.round(Math.min(1.5,Math.max(0.8,value))*100)/100});setZoom(theme.zoom);apply(theme);setError('')}catch(e){setError(`缩放未保存：${String(e)}`)}finally{setBusy(false)}}
 return <Popover title="界面与文字大小" content={<div><Space><Button disabled={busy||zoom<=0.8} onClick={()=>void set(zoom-0.1)}>缩小</Button><Button disabled={busy} onClick={()=>void set(1)}>重置 100%</Button><Button disabled={busy||zoom>=1.5} onClick={()=>void set(zoom+0.1)}>放大</Button></Space>{error&&<p role="alert">{error}</p>}</div>} trigger="click"><Button type="text">{Math.round(zoom*100)}%</Button></Popover>
}
