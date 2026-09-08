import React from "react";
import "./App.css";
import { HealthDashboard } from "../features/health/HealthDashboard";

export const App: React.FC = () => {
  return (
    <div className="min-h-screen bg-gray-950 text-gray-100 flex flex-col">
      <header className="border-b border-gray-800 bg-gray-900/50 backdrop-blur px-6 py-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="w-3 h-3 rounded-full bg-indigo-500 shadow-sm shadow-indigo-500/50" />
          <span className="font-bold text-lg tracking-tight text-white">OpenVideoAI</span>
          <span className="text-xs font-mono uppercase bg-indigo-950/80 text-indigo-400 px-2 py-0.5 rounded border border-indigo-800/60 ml-2">Phase 0</span>
        </div>
      </header>
      <main className="flex-1 py-6">
        <HealthDashboard />
      </main>
    </div>
  );
};

export default App;
