import {useEffect,useRef,useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {EditorState,Compartment} from '@codemirror/state'
import {EditorView,ViewPlugin,WidgetType,Decoration,keymap,placeholder,type DecorationSet} from '@codemirror/view'
import {defaultKeymap,history,historyKeymap} from '@codemirror/commands'
import {markdown} from '@codemirror/lang-markdown'
import {syntaxTree} from '@codemirror/language'
import ReactMarkdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import {Alert,Button,Dropdown,Segmented,Space,Tooltip} from 'antd'
import {BoldOutlined,ItalicOutlined,UnorderedListOutlined,LinkOutlined,PictureOutlined,MoreOutlined} from '@ant-design/icons'
import './MarkdownEditor.css'
export function normalizeLegacyImages(value:string):string{
 return value.replace(/\[图片:\s*([^\]]+)\]\s*\((file:\/\/\/[^)\r\n]+)\)/gi,(original,label,uri)=>{
  try{
   const pathname=new URL(uri).pathname
   const match=/\/writing-assets\/(.+)$/i.exec(pathname)
   if(!match)return original
   const relative=decodeURIComponent(match[1]).split(/[\\/]+/).filter(Boolean).map(segment=>encodeURIComponent(segment).replace(/[!'()*]/g,c=>`%${c.charCodeAt(0).toString(16).toUpperCase()}`)).join('/')
   const alt=String(label).trim().replace(/\\/g,'\\\\').replace(/\]/g,'\\]')
   return '!['+alt+'](writing-assets/'+relative+')'
  }catch{return original}
 })
}
class ImageWidget extends WidgetType {
 readonly path:string; readonly alt:string; constructor(path:string,alt:string){super();this.path=path;this.alt=alt}
 eq(other:ImageWidget){return other.path===this.path&&other.alt===this.alt}
 toDOM(){const wrap=document.createElement('span');wrap.className='inline-writing-image';const img=document.createElement('img');img.alt=this.alt;img.title='文档图片';wrap.appendChild(img);void invoke<string>('writing_image_read',{reference:this.path}).then(src=>{if(wrap.isConnected)img.src=src}).catch(()=>{wrap.textContent=`图片无法加载：${this.alt}`});return wrap}
 ignoreEvent(){return false}
}
function imageDecorations(view:EditorView):DecorationSet{
 const ranges:ReturnType<Decoration['range']>[]=[]
 for(const {from,to} of view.visibleRanges){syntaxTree(view.state).iterate({from,to,enter(node){if(node.name!=='Image')return;const text=view.state.sliceDoc(node.from,node.to);const match=/^!\[([^\]]*)\]\((writing-assets\/[A-Za-z0-9_./%\-]+)\)$/.exec(text);if(match)ranges.push(Decoration.replace({widget:new ImageWidget(match[2],match[1]||'图片')}).range(node.from,node.to))}})}
 return Decoration.set(ranges,true)
}
const inlineImages=ViewPlugin.fromClass(class {decorations:DecorationSet;constructor(view:EditorView){this.decorations=imageDecorations(view)} update(update:import('@codemirror/view').ViewUpdate){if(update.docChanged||update.viewportChanged||syntaxTree(update.startState)!==syntaxTree(update.state))this.decorations=imageDecorations(update.view)}},{decorations:v=>v.decorations,provide:plugin=>EditorView.atomicRanges.of(view=>view.plugin(plugin)?.decorations||Decoration.none)})
function PreviewImage({src,alt}:{src?:string;alt?:string}){const [resolved,setResolved]=useState<string|undefined>(),[error,setError]=useState(false);useEffect(()=>{let cancelled=false;setResolved(undefined);setError(false);if(!src)return;if(!src.startsWith('writing-assets/')){setResolved(src);return}void invoke<string>('writing_image_read',{reference:src}).then(url=>{if(!cancelled)setResolved(url)}).catch(()=>{if(!cancelled)setError(true)});return()=>{cancelled=true}},[src]);if(error)return <span role="img" aria-label={alt||'图片'}>图片无法加载：{alt||'图片'}</span>;return resolved?<img src={resolved} alt={alt||''}/>:<span>图片加载中…</span>}
type Props={value:string;onChange:(value:string)=>void;disabled?:boolean;label:string;rows?:number}
export default function MarkdownEditor({value,onChange,disabled=false,label,rows=20}:Props){
 const documentValue=normalizeLegacyImages(value)
 const parent=useRef<HTMLDivElement>(null),editor=useRef<EditorView|null>(null),latest=useRef(onChange),editable=useRef(new Compartment()),locked=useRef(false),disabledRef=useRef(disabled)
 const [imageError,setImageError]=useState(''),[importing,setImporting]=useState(false),[mode,setMode]=useState<'edit'|'preview'>('edit')
 latest.current=onChange;disabledRef.current=disabled
 function setLocked(view:EditorView,value:boolean){locked.current=value;setImporting(value);view.dispatch({effects:editable.current.reconfigure(EditorView.editable.of(!value&&!disabledRef.current))})}
 async function addImage(view:EditorView,source:'paste'|'pick',file?:File){
  if(locked.current||disabledRef.current)return
  if(file&&file.size>8*1024*1024){setImageError('图片不能超过 8 MiB');return}
  const selection=view.state.selection.main;setImageError('');setLocked(view,true)
  try{const path=source==='pick'?await invoke<string|null>('writing_image_import'):await invoke<string>('writing_image_paste',{bytes:Array.from(new Uint8Array(await file!.arrayBuffer()))});if(path&&editor.current===view){const snippet=`![图片](${path})`;view.dispatch({changes:{from:selection.from,to:selection.to,insert:snippet},selection:{anchor:selection.from+snippet.length}})}}catch(e){setImageError(`插入图片失败：${String(e)}`)}finally{if(editor.current===view){setLocked(view,false);view.focus()}else{locked.current=false;setImporting(false)}}
 }
 const addRef=useRef(addImage);addRef.current=addImage
 useEffect(()=>{if(mode!=='edit'||!parent.current)return;const view=new EditorView({parent:parent.current,state:EditorState.create({doc:documentValue,extensions:[placeholder('开始书写，或按 Ctrl+V 粘贴图片…'),history(),keymap.of([...defaultKeymap,...historyKeymap]),markdown(),inlineImages,EditorView.lineWrapping,editable.current.of(EditorView.editable.of(!disabledRef.current)),EditorView.contentAttributes.of({'aria-label':label,'role':'textbox','aria-multiline':'true'}),EditorView.updateListener.of(update=>{if(update.docChanged)latest.current(update.state.doc.toString())}),EditorView.domEventHandlers({paste(event,view){const item=Array.from(event.clipboardData?.items||[]).find(i=>i.kind==='file'&&i.type.startsWith('image/'));if(!item)return false;event.preventDefault();const file=item.getAsFile();if(!file){setImageError('无法读取剪贴板图片');return true}void addRef.current(view,'paste',file);return true}})]})});editor.current=view;return()=>{editor.current=null;view.destroy()}},[mode,label])
 useEffect(()=>{const view=editor.current;if(view&&view.state.doc.toString()!==documentValue)view.dispatch({changes:{from:0,to:view.state.doc.length,insert:documentValue}})},[documentValue])
 useEffect(()=>{const view=editor.current;if(view)view.dispatch({effects:editable.current.reconfigure(EditorView.editable.of(!disabled&&!locked.current))})},[disabled])
 function insert(before:string,after='',fallback='文本'){const view=editor.current;if(!view||disabled||locked.current)return;const {from,to}=view.state.selection.main;const selected=view.state.sliceDoc(from,to)||fallback;view.dispatch({changes:{from,to,insert:before+selected+after},selection:{anchor:from+before.length,head:from+before.length+selected.length}});view.focus()}
 return <div className="markdown-editor"><div className="markdown-toolbar"><Space size={2} wrap>{mode==='edit'&&<>
  <Dropdown trigger={['click']} menu={{items:[{key:'1',label:'一级标题'},{key:'2',label:'二级标题'},{key:'3',label:'三级标题'}],onClick:({key})=>insert('#'.repeat(Number(key))+' ','','标题')}}><Button disabled={disabled} size="small">标题 ▾</Button></Dropdown>
  <Tooltip title="粗体"><Button aria-label="粗体" disabled={disabled} size="small" icon={<BoldOutlined/>} onClick={()=>insert('**','**')}/></Tooltip>
  <Tooltip title="斜体"><Button aria-label="斜体" disabled={disabled} size="small" icon={<ItalicOutlined/>} onClick={()=>insert('*','*')}/></Tooltip>
  <Tooltip title="列表"><Button aria-label="列表" disabled={disabled} size="small" icon={<UnorderedListOutlined/>} onClick={()=>insert('- ','','列表项')}/></Tooltip>
  <Tooltip title="插入链接"><Button aria-label="链接" disabled={disabled} size="small" icon={<LinkOutlined/>} onClick={()=>insert('[','](https://example.com)','链接文字')}/></Tooltip>
  <Tooltip title="插入图片 · Ctrl+V"><Button aria-label="插入图片" disabled={disabled} loading={importing} size="small" icon={<PictureOutlined/>} onClick={()=>{if(editor.current)void addImage(editor.current,'pick')}}/></Tooltip>
  <Dropdown trigger={['click']} menu={{items:[{key:'quote',label:'引用'},{key:'inline',label:'行内代码'},{key:'code',label:'代码块'}],onClick:({key})=>{if(key==='quote')insert('> ','','引用');else if(key==='inline')insert('`','`','代码');else insert('```\n','\n```','代码')}}}><Button aria-label="更多格式" disabled={disabled} size="small" icon={<MoreOutlined/>}/></Dropdown>
 </>}</Space><Segmented size="small" aria-label="编辑或预览" disabled={importing} value={mode} onChange={v=>setMode(v as 'edit'|'preview')} options={[{value:'edit',label:'编辑'},{value:'preview',label:'预览'}]}/></div>{imageError&&<Alert type="error" message={imageError} closable onClose={()=>setImageError('')}/ >}{mode==='edit'?<div ref={parent} style={{minHeight:Math.max(340,rows*18)}}/>:<div className="markdown-preview" aria-label={`${label}预览`}><ReactMarkdown remarkPlugins={[remarkGfm]} components={{img:({src,alt})=><PreviewImage src={src} alt={alt}/>,a:({href,children})=><a href={href} target="_blank" rel="noopener noreferrer">{children}</a>}}>{documentValue}</ReactMarkdown></div>}</div>
}
