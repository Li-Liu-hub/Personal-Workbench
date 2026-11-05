import useUnsavedChanges from './useUnsavedChanges'
import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {Alert,Button,Card,InputNumber,Select,Slider,Space,Switch} from 'antd'
type Theme={accent:number[];text:number[];muted:number[];canvas:number[];sidebar:number[];surface:number[];input:number[];border:number[];button_text:number[];sidebar_text:number[];surface_opacity:number;sidebar_opacity:number;image_opacity:number;content_overlay:number;zoom:number;animations:boolean;background:string|null;fit:'Cover'|'Contain'|'Stretch'}
type ColorKey='accent'|'text'|'muted'|'canvas'|'sidebar'|'surface'|'input'|'border'|'button_text'|'sidebar_text'
const colors:[ColorKey,string][]=[['accent','主题色'],['text','正文文字'],['muted','次要文字'],['canvas','页面背景'],['sidebar','侧栏背景'],['surface','卡片与面板'],['input','输入框'],['border','边框'],['button_text','按钮文字'],['sidebar_text','侧栏文字']]
const hex=(v:number[])=>'#'+v.map(n=>n.toString(16).padStart(2,'0')).join('')
let previewSequence=0
function apply(theme:Theme){
 const root=document.documentElement;for(const [key] of colors)root.style.setProperty(`--desk-${key.replaceAll('_','-')}`,hex(theme[key]));
 const rgba=(rgb:number[],alpha:number)=>`rgba(${rgb.join(',')},${alpha})`
 root.style.setProperty('--desk-surface-alpha',rgba(theme.surface,theme.surface_opacity));root.style.setProperty('--desk-sidebar-alpha',rgba(theme.sidebar,theme.sidebar_opacity));root.style.setProperty('--desk-overlay',rgba(theme.canvas,theme.background?theme.content_overlay:1));root.style.setProperty('--desk-image-opacity',String(theme.image_opacity));root.style.setProperty('--desk-image-fit',theme.fit==='Stretch'?'100% 100%':theme.fit.toLowerCase());root.style.setProperty('--desk-zoom',String(theme.zoom));root.style.setProperty('--desk-motion',theme.animations?'0.16s':'0s');
 const sequence=++previewSequence;if(!theme.background)root.style.setProperty('--desk-background-image','none');else void invoke<string>('theme_image',{value:theme.background}).then(url=>{if(sequence===previewSequence)root.style.setProperty('--desk-background-image',`url("${url}")`)}).catch(()=>{if(sequence===previewSequence)root.style.setProperty('--desk-background-image','none')});
 window.dispatchEvent(new CustomEvent('personaldesk-theme',{detail:theme}))
}
export default function ThemeEditor({setWorkspaceDirty}:{setWorkspaceDirty?:(dirty:boolean)=>void}){
 const savedRef=useRef<Theme|null>(null)
 const [theme,setTheme]=useState<Theme|null>(null),[saved,setSaved]=useState<Theme|null>(null),[error,setError]=useState(''),[notice,setNotice]=useState('')
 useUnsavedChanges(JSON.stringify(theme)!==JSON.stringify(saved),setWorkspaceDirty)
 useEffect(()=>()=>{if(savedRef.current)apply(savedRef.current)},[])
 useEffect(()=>{let cancelled=false;void invoke<Theme>('theme_get').then(t=>{if(!cancelled){setTheme(t);setSaved(t);savedRef.current=t;apply(t)}}).catch(e=>{if(!cancelled)setError(`读取主题失败：${String(e)}`)});return()=>{cancelled=true}},[])
 function update(next:Theme){setTheme(next);apply(next);setNotice('')}
 async function save(){if(!theme)return;try{const persisted=await invoke<Theme>('theme_save',{theme});setTheme(persisted);setSaved(persisted);savedRef.current=persisted;apply(persisted);setError('');setNotice('主题已保存')}catch(e){setError(`保存主题失败：${String(e)}`)}}
 async function pick(){try{const path=await invoke<string|null>('theme_pick_image');if(path&&theme)update({...theme,background:path})}catch(e){setError(`选择背景失败：${String(e)}`)}}
 if(!theme)return error?<Alert type="error" message={error}/>:null
 return <Card title="编辑主题" style={{marginBottom:16}}>{error&&<Alert type="error" message={error}/>} {notice&&<Alert type="success" message={notice}/>}<div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(150px,1fr))',gap:12,margin:'16px 0'}}>{colors.map(([key,label])=><label key={key}>{label} <input aria-label={label} type="color" value={hex(theme[key])} onChange={e=>{const color=e.target.value;update({...theme,[key]:[1,3,5].map(i=>parseInt(color.slice(i,i+2),16))})}}/></label>)}</div><Space wrap><label>界面缩放 <InputNumber aria-label="界面缩放" min={0.8} max={1.5} step={0.05} value={theme.zoom} onChange={v=>{if(v!==null)update({...theme,zoom:v})}}/></label><label>启用动画 <Switch aria-label="启用动画" checked={theme.animations} onChange={animations=>update({...theme,animations})}/></label></Space><div style={{marginTop:16}}><Space wrap><Button onClick={()=>void pick()}>选择背景图片</Button><Button onClick={()=>update({...theme,background:null})}>移除背景</Button><span>{theme.background||'未设置背景'}</span><Select aria-label="背景显示方式" value={theme.fit} onChange={fit=>update({...theme,fit})} options={[{value:'Cover',label:'铺满裁切'},{value:'Contain',label:'完整显示'},{value:'Stretch',label:'拉伸'}]}/></Space></div><div style={{display:'grid',gridTemplateColumns:'repeat(auto-fit,minmax(180px,1fr))',gap:16,marginTop:16}}>{([['surface_opacity','面板不透明度'],['sidebar_opacity','侧栏不透明度'],['image_opacity','背景图片不透明度'],['content_overlay','内容遮罩'] ] as const).map(([key,label])=><label key={key}>{label}<Slider aria-label={label} min={0} max={1} step={0.01} value={theme[key]} onChange={value=>update({...theme,[key]:value})}/></label>)}</div><div style={{marginTop:16}}><Space><Button type="primary" onClick={()=>void save()}>保存主题</Button><Button onClick={()=>{if(saved){setTheme(saved);apply(saved);setNotice('已撤销未保存的修改')}}}>撤销修改</Button></Space></div></Card>
}
export {apply}
export type {Theme}
