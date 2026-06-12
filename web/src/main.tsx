import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import './styles.css';

type Health = {
  status: string;
  database: string;
};

type ExchangeInfo = {
  id: string;
  name: string;
  credential_schema: {
    required?: string[];
  };
  capabilities: string[];
};

type Credential = {
  id: string;
  exchange: string;
  name: string;
  has_payload: boolean;
  created_at: string;
  updated_at: string;
};

type Position = {
  position_id: string;
  product_id: string;
  direction: 'LONG' | 'SHORT' | null;
  volume: number;
  free_volume: number;
  position_price: number;
  closable_price: number;
  floating_profit: number;
  comment: string | null;
};

type Product = {
  datasource_id: string;
  product_id: string;
  name: string | null;
  quote_currency: string | null;
  base_currency: string | null;
  price_step: number | null;
  volume_step: number | null;
  allow_long: boolean | null;
  allow_short: boolean | null;
};

type Page = 'overview' | 'credentials' | 'positions' | 'products' | 'exchanges';

const pages: Array<{ id: Page; label: string; hint: string }> = [
  { id: 'overview', label: 'Overview', hint: 'Service and adapter status' },
  { id: 'credentials', label: 'Credentials', hint: 'Saved local metadata' },
  { id: 'positions', label: 'Positions', hint: 'Assets and open exposure' },
  { id: 'products', label: 'Products', hint: 'Exchange product specs' },
  { id: 'exchanges', label: 'Exchanges', hint: 'Schemas and capabilities' },
];

function useJson<T>(path: string) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    fetch(path)
      .then((response) => {
        if (!response.ok) {
          throw new Error(`${response.status} ${response.statusText}`);
        }
        return response.json() as Promise<T>;
      })
      .then((value) => {
        if (alive) {
          setData(value);
        }
      })
      .catch((caught: Error) => {
        if (alive) {
          setError(caught.message);
        }
      })
      .finally(() => {
        if (alive) {
          setLoading(false);
        }
      });

    return () => {
      alive = false;
    };
  }, [path]);

  return { data, error, loading };
}

function App() {
  const [page, setPage] = useState<Page>('overview');
  const health = useJson<Health>('/api/health');
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const credentials = useJson<Credential[]>('/api/credentials');
  const [selectedCredentialId, setSelectedCredentialId] = useState('');
  const [selectedExchangeId, setSelectedExchangeId] = useState('BINANCE');

  const selectedCredential = credentials.data?.find((item) => item.id === selectedCredentialId);
  const positionsPath = selectedCredentialId
    ? `/api/positions?credential_id=${encodeURIComponent(selectedCredentialId)}`
    : '/api/positions?credential_id=';
  const productsPath = `/api/products?exchange=${encodeURIComponent(selectedExchangeId)}`;
  const positions = useJson<Position[]>(positionsPath);
  const products = useJson<Product[]>(productsPath);

  useEffect(() => {
    if (!selectedCredentialId && credentials.data?.[0]) {
      setSelectedCredentialId(credentials.data[0].id);
    }
  }, [credentials.data, selectedCredentialId]);

  const exposure = useMemo(() => summarizePositions(positions.data ?? []), [positions.data]);

  const content = {
    overview: (
      <OverviewPage
        credentialCount={credentials.data?.length ?? 0}
        database={health.data?.database ?? '~/.1ex/1ex.sqlite3'}
        exchangeCount={exchanges.data?.length ?? 0}
        healthStatus={health.data?.status ?? 'checking'}
        positionCount={positions.data?.length ?? 0}
        productCount={products.data?.length ?? 0}
        selectedCredential={selectedCredential}
      />
    ),
    credentials: <CredentialsPage credentials={credentials.data ?? []} exchanges={exchanges.data ?? []} />,
    positions: (
      <PositionsPage
        credentials={credentials.data ?? []}
        error={selectedCredentialId ? positions.error : null}
        exposure={exposure}
        loading={selectedCredentialId ? positions.loading : false}
        positions={selectedCredentialId ? positions.data ?? [] : []}
        selectedCredentialId={selectedCredentialId}
        onSelectCredential={setSelectedCredentialId}
      />
    ),
    products: (
      <ProductsPage
        error={products.error}
        exchanges={exchanges.data ?? []}
        loading={products.loading}
        products={products.data ?? []}
        selectedExchangeId={selectedExchangeId}
        onSelectExchange={setSelectedExchangeId}
      />
    ),
    exchanges: <ExchangesPage exchanges={exchanges.data ?? []} />,
  }[page];

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Primary navigation">
        <div className="brand-block">
          <span className="brand-mark">1E</span>
          <div>
            <strong>1Exchange</strong>
            <span>Local account console</span>
          </div>
        </div>
        <nav className="nav-list">
          {pages.map((item) => (
            <button
              aria-current={page === item.id ? 'page' : undefined}
              className="nav-item"
              key={item.id}
              onClick={() => setPage(item.id)}
              type="button"
            >
              <span>{item.label}</span>
              <small>{item.hint}</small>
            </button>
          ))}
        </nav>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p className="section-label">{currentPage(page).label}</p>
            <h1>{currentPage(page).hint}</h1>
          </div>
          <div className="status-pill" data-state={health.data?.status === 'ok' ? 'ok' : 'pending'}>
            <span /> API {health.data?.status ?? 'checking'}
          </div>
        </header>
        {content}
      </main>
    </div>
  );
}

function OverviewPage(props: {
  credentialCount: number;
  database: string;
  exchangeCount: number;
  healthStatus: string;
  positionCount: number;
  productCount: number;
  selectedCredential?: Credential;
}) {
  return (
    <div className="page-stack">
      <section className="metrics-grid" aria-label="System summary">
        <Metric label="API" value={props.healthStatus} tone={props.healthStatus === 'ok' ? 'good' : 'neutral'} />
        <Metric label="Exchanges" value={props.exchangeCount.toString()} />
        <Metric label="Credentials" value={props.credentialCount.toString()} />
        <Metric label="Loaded positions" value={props.positionCount.toString()} />
      </section>

      <section className="panel split-panel">
        <div>
          <p className="section-label">Active context</p>
          <h2>{props.selectedCredential?.name ?? 'No credential selected'}</h2>
          <p className="muted">
            {props.selectedCredential
              ? `${props.selectedCredential.exchange} credential metadata is loaded. Payload stays server-side.`
              : 'Create or select a credential to load account and position data.'}
          </p>
        </div>
        <dl className="detail-list">
          <div>
            <dt>SQLite</dt>
            <dd>{props.database}</dd>
          </div>
          <div>
            <dt>Product rows</dt>
            <dd>{props.productCount}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function CredentialsPage(props: { credentials: Credential[]; exchanges: ExchangeInfo[] }) {
  return (
    <div className="page-stack">
      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Saved locally</p>
            <h2>Credentials</h2>
          </div>
          <span className="count-chip">{props.credentials.length} saved</span>
        </div>
        <DataTable
          empty="No credentials yet. POST /api/credentials or use the API directly to add one."
          headers={['Name', 'Exchange', 'Payload', 'Created', 'ID']}
          rows={props.credentials.map((item) => [
            item.name,
            <Badge key="exchange">{item.exchange}</Badge>,
            item.has_payload ? 'Stored' : 'Missing',
            formatDate(item.created_at),
            <code key="id">{item.id}</code>,
          ])}
        />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Required payload fields</p>
            <h2>Credential schemas</h2>
          </div>
        </div>
        <div className="schema-grid">
          {props.exchanges.map((exchange) => (
            <div className="schema-row" key={exchange.id}>
              <strong>{exchange.id}</strong>
              <span>{exchange.credential_schema.required?.join(', ') || 'No required fields'}</span>
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function PositionsPage(props: {
  credentials: Credential[];
  error: string | null;
  exposure: { total: number; long: number; short: number; assets: number };
  loading: boolean;
  positions: Position[];
  selectedCredentialId: string;
  onSelectCredential: (value: string) => void;
}) {
  return (
    <div className="page-stack">
      <section className="toolbar-panel">
        <label>
          Credential
          <select value={props.selectedCredentialId} onChange={(event) => props.onSelectCredential(event.target.value)}>
            <option value="">Select credential</option>
            {props.credentials.map((credential) => (
              <option key={credential.id} value={credential.id}>
                {credential.exchange} · {credential.name}
              </option>
            ))}
          </select>
        </label>
      </section>

      <section className="metrics-grid compact" aria-label="Position summary">
        <Metric label="Rows" value={props.exposure.total.toString()} />
        <Metric label="Assets" value={props.exposure.assets.toString()} />
        <Metric label="Long" value={props.exposure.long.toString()} tone="good" />
        <Metric label="Short" value={props.exposure.short.toString()} tone="warn" />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Live read</p>
            <h2>Positions</h2>
          </div>
          {props.loading ? <span className="count-chip">Loading</span> : <span className="count-chip">{props.positions.length} rows</span>}
        </div>
        <InlineError message={props.error} />
        <DataTable
          empty="Select a credential to load positions. If a request fails, check API permissions on the exchange key."
          headers={['Position', 'Product', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'P/L']}
          rows={props.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            <Value key="pnl" value={item.floating_profit} />,
          ])}
        />
      </section>
    </div>
  );
}

function ProductsPage(props: {
  error: string | null;
  exchanges: ExchangeInfo[];
  loading: boolean;
  products: Product[];
  selectedExchangeId: string;
  onSelectExchange: (value: string) => void;
}) {
  return (
    <div className="page-stack">
      <section className="toolbar-panel">
        <label>
          Exchange
          <select value={props.selectedExchangeId} onChange={(event) => props.onSelectExchange(event.target.value)}>
            {props.exchanges.map((exchange) => (
              <option key={exchange.id} value={exchange.id}>
                {exchange.id} · {exchange.name}
              </option>
            ))}
          </select>
        </label>
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Public specs</p>
            <h2>Products</h2>
          </div>
          {props.loading ? <span className="count-chip">Loading</span> : <span className="count-chip">{props.products.length} rows</span>}
        </div>
        <InlineError message={props.error} />
        <DataTable
          empty="No products returned for this exchange."
          headers={['Product', 'Name', 'Base', 'Quote', 'Price step', 'Volume step', 'Sides']}
          rows={props.products.slice(0, 250).map((item) => [
            <code key="product">{item.product_id}</code>,
            item.name ?? '-',
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            formatOptionalNumber(item.price_step),
            formatOptionalNumber(item.volume_step),
            sideLabel(item),
          ])}
        />
      </section>
    </div>
  );
}

function ExchangesPage(props: { exchanges: ExchangeInfo[] }) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Adapter registry</p>
          <h2>Supported exchanges</h2>
        </div>
        <span className="count-chip">{props.exchanges.length} adapters</span>
      </div>
      <DataTable
        empty="No exchange adapters registered."
        headers={['ID', 'Name', 'Capabilities', 'Credential fields']}
        rows={props.exchanges.map((item) => [
          <Badge key="id">{item.id}</Badge>,
          item.name,
          item.capabilities.join(', '),
          item.credential_schema.required?.join(', ') || '-',
        ])}
      />
    </section>
  );
}

function Metric(props: { label: string; value: string; tone?: 'neutral' | 'good' | 'warn' }) {
  return (
    <div className="metric" data-tone={props.tone ?? 'neutral'}>
      <span>{props.label}</span>
      <strong>{props.value}</strong>
    </div>
  );
}

function DataTable(props: { empty: string; headers: string[]; rows: React.ReactNode[][] }) {
  if (props.rows.length === 0) {
    return <div className="empty-state">{props.empty}</div>;
  }

  return (
    <div className="table-wrap">
      <table>
        <thead>
          <tr>
            {props.headers.map((header) => (
              <th key={header}>{header}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {props.rows.map((row, rowIndex) => (
            <tr key={rowIndex}>
              {row.map((cell, cellIndex) => (
                <td key={cellIndex}>{cell}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Badge(props: { children: React.ReactNode }) {
  return <span className="badge">{props.children}</span>;
}

function InlineError(props: { message: string | null }) {
  return props.message ? <div className="inline-error">{props.message}</div> : null;
}

function Value(props: { value: number }) {
  const tone = props.value > 0 ? 'positive' : props.value < 0 ? 'negative' : 'flat';
  return <span className="value" data-tone={tone}>{formatNumber(props.value)}</span>;
}

function currentPage(page: Page) {
  return pages.find((item) => item.id === page) ?? pages[0];
}

function summarizePositions(positions: Position[]) {
  return positions.reduce(
    (summary, item) => ({
      total: summary.total + 1,
      assets: summary.assets + (item.direction ? 0 : 1),
      long: summary.long + (item.direction === 'LONG' ? 1 : 0),
      short: summary.short + (item.direction === 'SHORT' ? 1 : 0),
    }),
    { total: 0, assets: 0, long: 0, short: 0 },
  );
}

function formatDate(value: string) {
  return new Date(value).toLocaleString();
}

function formatNumber(value: number) {
  return Intl.NumberFormat(undefined, { maximumFractionDigits: 8 }).format(value);
}

function formatOptionalNumber(value: number | null) {
  return value === null ? '-' : formatNumber(value);
}

function sideLabel(product: Product) {
  if (product.allow_long && product.allow_short) {
    return 'Long / Short';
  }
  if (product.allow_long) {
    return 'Long';
  }
  return '-';
}

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
