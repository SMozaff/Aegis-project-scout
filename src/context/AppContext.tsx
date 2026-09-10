import { createContext, useContext, useState, type ReactNode } from 'react';
import type { AppSettings, ScanReport, ScanProgress } from '../types';

interface AppState {
  settings: AppSettings;
  setSettings: (s: AppSettings) => void;
  report: ScanReport | null;
  setReport: (r: ScanReport | null) => void;
  progress: ScanProgress | null;
  setProgress: (p: ScanProgress | null) => void;
}

const Context = createContext<AppState | null>(null);

export function AppProvider({children}:{children:ReactNode}) {
 const [settings,setSettings]=useState<AppSettings>({languages:['TypeScript'],lookback_days:30,max_repositories:10,health_check:true,max_files_per_repository:25});
 const [report,setReport]=useState<ScanReport|null>(null);
 const [progress,setProgress]=useState<ScanProgress|null>(null);
 return <Context.Provider value={{settings,setSettings,report,setReport,progress,setProgress}}>{children}</Context.Provider>;
}

export function useAppState(){ const v=useContext(Context); if(!v) throw new Error('AppProvider missing'); return v; }
