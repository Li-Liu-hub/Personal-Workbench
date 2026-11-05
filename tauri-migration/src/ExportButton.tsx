import {useState} from 'react'
import {invoke} from '@tauri-apps/api/core'
import {Alert,Button,Modal,Space} from 'antd'
type Scope='essays'|'documents'|'contacts'|'websites'|'inventory'|'credentials'|'folders'|'tools'
const labels:Record<Scope,string>={essays:'每日随笔',documents:'文档',contacts:'联系人',websites:'网站收藏',inventory:'物品清单',credentials:'账号密码',folders:'快捷路径',tools:'小工具'}
export default function ExportButton({scope,dirty=false}:{scope:Scope;dirty?:boolean}){
 const [open,setOpen]=useState(false),[busy,setBusy]=useState(false),[notice,setNotice]=useState(''),[error,setError]=useState('')
 async function run(selected:Scope|'all'){
  if(dirty&&!window.confirm('当前编辑内容尚未正式保存，导出内容只包含已保存的数据。继续导出吗？'))return
  setBusy(true);try{const result=await invoke<{path:string;warnings:number}|null>('export_test_data',{scope:selected});if(result){setNotice(`已导出到 ${result.path}${result.warnings?`；${result.warnings} 项需要关注`:''}`);setOpen(false)}setError('')}catch(e){setError(`导出失败：${String(e)}`)}finally{setBusy(false)}
 }
 return <><Button onClick={()=>setOpen(true)}>导出</Button><Modal title="导出数据" open={open} onCancel={()=>setOpen(false)} footer={null}><p>选择导出内容，保存为 ZIP 压缩包。</p>{error&&<Alert type="error" message={error}/>}<Space style={{marginTop:16}}><Button loading={busy} onClick={()=>void run(scope)}>导出{labels[scope]}</Button><Button type="primary" loading={busy} onClick={()=>void run('all')}>导出全部数据</Button></Space></Modal><Modal title="导出结果" open={!!notice} onCancel={()=>setNotice('')} footer={<Button onClick={()=>setNotice('')}>完成</Button>}><p>{notice}</p></Modal></>
}
