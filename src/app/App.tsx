import React from "react";
import "./App.css";

export const App: React.FC = () => {
  return (
    <div className="app-container">
      <header className="app-header">
        <div className="brand">
          <span className="brand-title">OpenVideoAI</span>
        </div>
      </header>
      <main className="app-main">
        <p>OpenVideoAI Studio Foundation</p>
      </main>
    </div>
  );
};

export default App;
