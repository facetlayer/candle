import { useState } from 'react';
import { ServiceList } from './components/ServiceList';
import { LogViewer } from './components/LogViewer';
import './App.css';

export interface SelectedService {
  serviceName: string;
  projectDir: string;
}

export function App() {
  const [selected, setSelected] = useState<SelectedService | null>(null);

  return (
    <div className="app">
      <header className="app-header">
        <h1>Candle</h1>
        <span className="subtitle">Process Manager</span>
      </header>
      <div className="app-body">
        <ServiceList
          selected={selected}
          onSelect={setSelected}
        />
        <LogViewer
          serviceName={selected?.serviceName ?? null}
          projectDir={selected?.projectDir ?? null}
        />
      </div>
    </div>
  );
}
