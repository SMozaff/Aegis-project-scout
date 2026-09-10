import { useState } from 'react';
import { useAppState } from '../context/AppContext';
import { api } from '../lib/tauri';

const stacks=['OpenAI','Anthropic','Gemini','FastAPI','Node','React','Rust'];
export function Scanner(){
 const {settings,setSettings,setReport,setProgress}=useAppState();
 const [token,setToken]=useState(''); const [selected,setSelected]=useState(stacks.slice(0,3)); const [running,setRunning]=useState(false);
 async function scan(){setRunning(true);setProgress({stage:'discovering',message:'Scanning repositories',completed:0,total:1}); try{setReport(await api.runScan({...settings,github_token:token||null,languages:selected}));}finally{setRunning(false)}}
 return <div className="space-y-5"><h1 className="text-xl text-white">Scanner</h1><input className="w-full rounded bg-slate-900 p-3" type="password" placeholder="GitHub token" value={token} onChange={e=>setToken(e.target.value)}/><div className="grid grid-cols-2 gap-2">{stacks.map(s=><label className="text-slate-300" key={s}><input type="checkbox" checked={selected.includes(s)} onChange={()=>setSelected(selected.includes(s)?selected.filter(x=>x!==s):[...selected,s])}/> {s}</label>)}</div><input type="range" min="1" max="365" value={settings.lookback_days} onChange={e=>setSettings({...settings,lookback_days:Number(e.target.value)})}/><button disabled={running} onClick={scan} className="rounded bg-sky-600 px-4 py-2">{running?'Running':'Start Scan'}</button></div>
}
