import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

type Health = {
  status: string;
  database: string;
};

function App() {
  const [health, setHealth] = useState<Health | null>(null);

  useEffect(() => {
    fetch('/api/health')
      .then((response) => response.json())
      .then(setHealth);
  }, []);

  return (
    <main className="shell">
      <section className="hero">
        <p className="eyebrow">One Exchange for All Accounts</p>
        <h1>1Exchange</h1>
        <p className="summary">本地运行的多交易所账户、资产和持仓统一视图。</p>
      </section>

      <section className="panel">
        <h2>服务状态</h2>
        <dl>
          <div>
            <dt>API</dt>
            <dd>{health?.status ?? '连接中'}</dd>
          </div>
          <div>
            <dt>SQLite</dt>
            <dd>{health?.database ?? '~/.1ex/1ex.sqlite3'}</dd>
          </div>
        </dl>
      </section>
    </main>
  );
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
