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
  notional_value: number;
  notional_currency: string | null;
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

type PortfolioAccount = {
  credential: Credential;
  error: string | null;
  positions: Position[];
};

type Page = 'overview' | 'portfolio' | 'trade' | 'credentials' | 'positions' | 'products' | 'exchanges';

const pages: Array<{ id: Page; label: string; hint: string }> = [
  { id: 'overview', label: 'Overview', hint: 'Service and adapter status' },
  { id: 'portfolio', label: 'Portfolio', hint: 'All credential assets' },
  { id: 'trade', label: 'Trade', hint: 'Trading board' },
  { id: 'credentials', label: 'Credentials', hint: 'Saved local metadata' },
  { id: 'positions', label: 'Positions', hint: 'Assets and open exposure' },
  { id: 'products', label: 'Products', hint: 'Exchange product specs' },
  { id: 'exchanges', label: 'Exchanges', hint: 'Schemas and capabilities' },
];

const emptyCredentials: Credential[] = [];

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

function usePortfolio(credentials: Credential[]) {
  const [accounts, setAccounts] = useState<PortfolioAccount[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    setLoading(credentials.length > 0);
    setAccounts([]);

    Promise.all(
      credentials.map(async (credential) => {
        try {
          const response = await fetch(`/api/positions?credential_id=${encodeURIComponent(credential.id)}`);
          if (!response.ok) {
            throw new Error(`${response.status} ${response.statusText}`);
          }
          return { credential, error: null, positions: (await response.json()) as Position[] };
        } catch (caught) {
          return { credential, error: (caught as Error).message, positions: [] };
        }
      }),
    )
      .then((nextAccounts) => {
        if (alive) {
          setAccounts(nextAccounts);
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
  }, [credentials]);

  return { accounts, loading };
}

function App() {
  const [page, setPage] = useState<Page>('overview');
  const health = useJson<Health>('/api/health');
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const credentials = useJson<Credential[]>('/api/credentials');
  const credentialList = credentials.data ?? emptyCredentials;
  const [selectedCredentialId, setSelectedCredentialId] = useState('');
  const [selectedExchangeId, setSelectedExchangeId] = useState('BINANCE');
  const [selectedTradeProductId, setSelectedTradeProductId] = useState('');

  const selectedCredential = credentials.data?.find((item) => item.id === selectedCredentialId);
  const positionsPath = selectedCredentialId
    ? `/api/positions?credential_id=${encodeURIComponent(selectedCredentialId)}`
    : '/api/positions?credential_id=';
  const productsPath = `/api/products?exchange=${encodeURIComponent(selectedExchangeId)}`;
  const positions = useJson<Position[]>(positionsPath);
  const products = useJson<Product[]>(productsPath);
  const portfolio = usePortfolio(credentialList);

  useEffect(() => {
    if (!selectedCredentialId && credentials.data?.[0]) {
      setSelectedCredentialId(credentials.data[0].id);
    }
  }, [credentials.data, selectedCredentialId]);

  useEffect(() => {
    if (products.data?.length && !products.data.some((item) => item.product_id === selectedTradeProductId)) {
      setSelectedTradeProductId(products.data[0].product_id);
    }
  }, [products.data, selectedTradeProductId]);

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
    portfolio: <PortfolioPage accounts={portfolio.accounts} loading={portfolio.loading} />,
    trade: (
      <TradePage
        credentials={credentialList}
        error={selectedCredentialId ? positions.error : null}
        exchanges={exchanges.data ?? []}
        loading={selectedCredentialId ? positions.loading || products.loading : products.loading}
        positions={selectedCredentialId ? positions.data ?? [] : []}
        products={products.data ?? []}
        selectedCredentialId={selectedCredentialId}
        selectedExchangeId={selectedExchangeId}
        selectedProductId={selectedTradeProductId}
        onSelectCredential={setSelectedCredentialId}
        onSelectExchange={setSelectedExchangeId}
        onSelectProduct={setSelectedTradeProductId}
      />
    ),
    credentials: <CredentialsPage credentials={credentialList} exchanges={exchanges.data ?? []} />,
    positions: (
      <PositionsPage
        credentials={credentialList}
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

function PortfolioPage(props: { accounts: PortfolioAccount[]; loading: boolean }) {
  const summary = summarizePortfolio(props.accounts);
  const assetRows = summarizePortfolioAssets(props.accounts);

  return (
    <div className="page-stack">
      <section className="metrics-grid compact" aria-label="Portfolio summary">
        <Metric label="Credentials" value={summary.credentials.toString()} />
        <Metric label="Loaded positions" value={summary.positions.toString()} />
        <Metric label="Notional value" value={formatNotionalBreakdown(summary.notionalByCurrency)} />
        <Metric label="Floating P/L" value={formatNumber(summary.pnl)} tone={summary.pnl < 0 ? 'warn' : 'good'} />
        <Metric label="Read errors" value={summary.errors.toString()} tone={summary.errors > 0 ? 'warn' : 'neutral'} />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">By credential</p>
            <h2>Account summary</h2>
          </div>
          <span className="count-chip">{props.loading ? 'Loading' : `${props.accounts.length} credentials`}</span>
        </div>
        <DataTable
          empty="No credentials yet. Add credentials before loading a portfolio summary."
          headers={['Credential', 'Exchange', 'Positions', 'Assets', 'Long', 'Short', 'Notional value', 'Floating P/L', 'Status']}
          rows={props.accounts.map((account) => {
            const accountSummary = summarizeAccountPositions(account.positions);
            return [
              account.credential.name,
              <Badge key="exchange">{account.credential.exchange}</Badge>,
              accountSummary.total.toString(),
              accountSummary.assets.toString(),
              accountSummary.long.toString(),
              accountSummary.short.toString(),
              formatNotionalBreakdown(accountSummary.notionalByCurrency),
              <Value key="pnl" value={accountSummary.pnl} />,
              account.error ? <span className="status-text bad" key="status">{account.error}</span> : <span className="status-text good" key="status">Loaded</span>,
            ];
          })}
        />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">By product</p>
            <h2>Asset exposure</h2>
          </div>
          <span className="count-chip">{assetRows.length} products</span>
        </div>
        <DataTable
          empty="No positions were loaded from saved credentials."
          headers={['Product', 'Currency', 'Credentials', 'Rows', 'Volume', 'Free', 'Notional value', 'Floating P/L']}
          rows={assetRows.map((row) => [
            <code key="product">{row.productId}</code>,
            row.currency,
            row.credentials.toString(),
            row.rows.toString(),
            formatNumber(row.volume),
            formatNumber(row.freeVolume),
            formatNumber(row.notionalValue),
            <Value key="pnl" value={row.pnl} />,
          ])}
        />
      </section>
    </div>
  );
}

function TradePage(props: {
  credentials: Credential[];
  error: string | null;
  exchanges: ExchangeInfo[];
  loading: boolean;
  positions: Position[];
  products: Product[];
  selectedCredentialId: string;
  selectedExchangeId: string;
  selectedProductId: string;
  onSelectCredential: (value: string) => void;
  onSelectExchange: (value: string) => void;
  onSelectProduct: (value: string) => void;
}) {
  const selectedProduct = props.products.find((item) => item.product_id === props.selectedProductId);
  const relatedPositions = props.positions.filter((item) => item.product_id === props.selectedProductId);
  const quote = boardQuote(selectedProduct, relatedPositions);
  const book = mockOrderBook(quote.mark);
  const risk = summarizeTradeRisk(props.positions);

  return (
    <div className="trade-board">
      <section className="trade-toolbar">
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
        <label className="wide-control">
          Product
          <select value={props.selectedProductId} onChange={(event) => props.onSelectProduct(event.target.value)}>
            <option value="">Select product</option>
            {props.products.slice(0, 500).map((product) => (
              <option key={product.product_id} value={product.product_id}>
                {product.product_id}
              </option>
            ))}
          </select>
        </label>
      </section>

      <InlineError message={props.error} />

      <section className="chart-panel trade-panel">
        <PanelTitle label="Chart" title={selectedProduct?.name ?? 'Select a product'} action={props.loading ? 'Loading' : 'Read only'} />
        <div className="chart-shell">
          <div className="chart-gridlines" />
          <svg className="price-line" viewBox="0 0 640 220" preserveAspectRatio="none" aria-hidden="true">
            <path d="M0 168 C60 120 96 132 144 102 S248 76 312 114 414 170 480 104 574 62 640 88" />
          </svg>
          <div className="chart-readout">
            <span>{selectedProduct?.product_id ?? 'No product selected'}</span>
            <strong>{formatNumber(quote.mark)}</strong>
            <small>synthetic board price from current position/product context</small>
          </div>
        </div>
      </section>

      <section className="book-panel trade-panel">
        <PanelTitle label="Order book" title="Depth" action="Placeholder" />
        <OrderBookPreview asks={book.asks} bids={book.bids} />
      </section>

      <section className="account-panel trade-panel">
        <PanelTitle label="Account" title="Positions" action={`${relatedPositions.length} related`} />
        <DataTable
          empty="No position matches the selected product."
          headers={['Position', 'Side', 'Volume', 'Entry', 'Mark', 'Notional', 'P/L']}
          rows={(relatedPositions.length ? relatedPositions : props.positions.slice(0, 8)).map((item) => [
            item.position_id,
            item.direction ? <Badge key="side">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatNumber(notionalValue(item)),
            <Value key="pnl" value={item.floating_profit} />,
          ])}
        />
      </section>

      <section className="manual-panel trade-panel">
        <PanelTitle label="Manual trade" title="Order draft" action="Disabled" />
        <ManualTradeDraft product={selectedProduct} selectedCredentialId={props.selectedCredentialId} />
      </section>

      <section className="profit-panel trade-panel">
        <PanelTitle label="Risk" title="Account summary" action="Positions" />
        <dl className="risk-list">
          <div>
            <dt>Total rows</dt>
            <dd>{risk.total}</dd>
          </div>
          <div>
            <dt>Floating P/L</dt>
            <dd><Value value={risk.pnl} /></dd>
          </div>
          <div>
            <dt>Long positions</dt>
            <dd>{risk.long}</dd>
          </div>
          <div>
            <dt>Short positions</dt>
            <dd>{risk.short}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function PanelTitle(props: { action?: string; label: string; title: string }) {
  return (
    <div className="trade-panel-title">
      <div>
        <p className="section-label">{props.label}</p>
        <h2>{props.title}</h2>
      </div>
      {props.action ? <span className="count-chip">{props.action}</span> : null}
    </div>
  );
}

function OrderBookPreview(props: { asks: Array<[number, number]>; bids: Array<[number, number]> }) {
  return (
    <div className="order-book-preview">
      <div className="book-header"><span>Price</span><span>Size</span><span>Total</span></div>
      <div className="book-side asks">
        {props.asks.map(([price, size], index) => (
          <BookRow key={`ask-${price}`} price={price} size={size} total={size * (index + 1)} side="ask" />
        ))}
      </div>
      <div className="book-spread">Spread {formatNumber(Math.abs(props.asks.at(-1)![0] - props.bids[0][0]))}</div>
      <div className="book-side bids">
        {props.bids.map(([price, size], index) => (
          <BookRow key={`bid-${price}`} price={price} size={size} total={size * (index + 1)} side="bid" />
        ))}
      </div>
    </div>
  );
}

function BookRow(props: { price: number; side: 'ask' | 'bid'; size: number; total: number }) {
  return (
    <div className="book-row" data-side={props.side}>
      <span>{formatNumber(props.price)}</span>
      <span>{formatNumber(props.size)}</span>
      <span>{formatNumber(props.total)}</span>
    </div>
  );
}

function ManualTradeDraft(props: { product?: Product; selectedCredentialId: string }) {
  const [intent, setIntent] = useState<'OPEN' | 'CLOSE'>('OPEN');
  const [orderType, setOrderType] = useState<'LIMIT' | 'MARKET' | 'STOP'>('LIMIT');

  return (
    <form className="trade-form" onSubmit={(event) => event.preventDefault()}>
      <div className="segmented-control" aria-label="Open or close">
        {(['OPEN', 'CLOSE'] as const).map((value) => (
          <button className={intent === value ? 'active' : ''} key={value} onClick={() => setIntent(value)} type="button">
            {value === 'OPEN' ? 'Open' : 'Close'}
          </button>
        ))}
      </div>
      <div className="segmented-control" aria-label="Order type">
        {(['LIMIT', 'MARKET', 'STOP'] as const).map((value) => (
          <button className={orderType === value ? 'active' : ''} key={value} onClick={() => setOrderType(value)} type="button">
            {value}
          </button>
        ))}
      </div>
      <label>
        Price
        <input disabled={orderType === 'MARKET'} inputMode="decimal" placeholder={orderType === 'MARKET' ? 'Market' : '0.00'} />
      </label>
      <label>
        Volume
        <input inputMode="decimal" placeholder={formatOptionalNumber(props.product?.volume_step ?? null)} />
      </label>
      <div className="trade-actions">
        <button className="buy-action" disabled={!props.selectedCredentialId || !props.product} type="button">
          {intent === 'OPEN' ? 'Open long' : 'Close short'}
        </button>
        <button className="sell-action" disabled={!props.selectedCredentialId || !props.product} type="button">
          {intent === 'OPEN' ? 'Open short' : 'Close long'}
        </button>
      </div>
      <p className="form-note">Order submission is not enabled in 1Exchange yet. This panel mirrors Yuan's manual trade workflow for layout and review.</p>
    </form>
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
          headers={['Position', 'Product', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'Notional', 'P/L']}
          rows={props.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatNumber(notionalValue(item)),
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

function summarizePortfolio(accounts: PortfolioAccount[]) {
  return accounts.reduce(
    (summary, account) => {
      const accountSummary = summarizeAccountPositions(account.positions);
      mergeCurrencyTotals(summary.notionalByCurrency, accountSummary.notionalByCurrency);
      return {
        credentials: summary.credentials + 1,
        errors: summary.errors + (account.error ? 1 : 0),
        positions: summary.positions + accountSummary.total,
        notionalByCurrency: summary.notionalByCurrency,
        pnl: summary.pnl + accountSummary.pnl,
      };
    },
    { credentials: 0, errors: 0, positions: 0, notionalByCurrency: new Map<string, number>(), pnl: 0 },
  );
}

function summarizeAccountPositions(positions: Position[]) {
  return positions.reduce(
    (summary, item) => {
      addCurrencyTotal(summary.notionalByCurrency, item.notional_currency, item.notional_value);
      return {
        total: summary.total + 1,
        assets: summary.assets + (item.direction ? 0 : 1),
        long: summary.long + (item.direction === 'LONG' ? 1 : 0),
        short: summary.short + (item.direction === 'SHORT' ? 1 : 0),
        notionalByCurrency: summary.notionalByCurrency,
        pnl: summary.pnl + item.floating_profit,
      };
    },
    { total: 0, assets: 0, long: 0, short: 0, notionalByCurrency: new Map<string, number>(), pnl: 0 },
  );
}

function summarizePortfolioAssets(accounts: PortfolioAccount[]) {
  const rows = new Map<
    string,
    { credentialIds: Set<string>; currency: string; freeVolume: number; notionalValue: number; pnl: number; productId: string; rows: number; volume: number }
  >();
  for (const account of accounts) {
    for (const position of account.positions) {
      const currency = position.notional_currency ?? 'UNKNOWN';
      const rowKey = `${position.product_id}\u0000${currency}`;
      const current = rows.get(rowKey) ?? {
        credentialIds: new Set<string>(),
        currency,
        freeVolume: 0,
        notionalValue: 0,
        pnl: 0,
        productId: position.product_id,
        rows: 0,
        volume: 0,
      };
      current.credentialIds.add(account.credential.id);
      current.rows += 1;
      current.volume += finiteNumber(position.volume);
      current.freeVolume += finiteNumber(position.free_volume);
      current.notionalValue += finiteNumber(position.notional_value);
      current.pnl += finiteNumber(position.floating_profit);
      rows.set(rowKey, current);
    }
  }

  return Array.from(rows.values())
    .map((row) => ({ ...row, credentials: row.credentialIds.size }))
    .sort((a, b) => Math.abs(b.notionalValue) - Math.abs(a.notionalValue));
}

function notionalValue(position: Position) {
  return position.notional_value;
}

function addCurrencyTotal(totals: Map<string, number>, currency: string | null, value: number) {
  const key = currency ?? 'UNKNOWN';
  totals.set(key, (totals.get(key) ?? 0) + finiteNumber(value));
}

function mergeCurrencyTotals(target: Map<string, number>, source: Map<string, number>) {
  for (const [currency, value] of source.entries()) {
    addCurrencyTotal(target, currency, value);
  }
}

function formatNotionalBreakdown(totals: Map<string, number>) {
  const rows = Array.from(totals.entries()).filter(([, value]) => value !== 0);
  if (rows.length === 0) {
    return '0';
  }
  return rows
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([currency, value]) => `${formatNumber(value)} ${currency}`)
    .join(' / ');
}

function summarizeTradeRisk(positions: Position[]) {
  return positions.reduce(
    (summary, item) => ({
      total: summary.total + 1,
      pnl: summary.pnl + finiteNumber(item.floating_profit),
      long: summary.long + (item.direction === 'LONG' ? 1 : 0),
      short: summary.short + (item.direction === 'SHORT' ? 1 : 0),
    }),
    { total: 0, pnl: 0, long: 0, short: 0 },
  );
}

function boardQuote(product: Product | undefined, positions: Position[]) {
  const positionMark = positions.find((item) => Number.isFinite(item.closable_price) && item.closable_price > 0)?.closable_price;
  const productStep = product?.price_step && product.price_step > 0 ? product.price_step * 10_000 : 1;
  return { mark: positionMark ?? productStep };
}

function mockOrderBook(mark: number) {
  const base = mark > 0 ? mark : 1;
  const step = base >= 100 ? 0.5 : 0.01;
  return {
    asks: Array.from({ length: 8 }, (_, index) => [base + step * (8 - index), (index + 1) * 1.7] as [number, number]),
    bids: Array.from({ length: 8 }, (_, index) => [base - step * (index + 1), (index + 1) * 1.5] as [number, number]),
  };
}

function formatDate(value: string) {
  return new Date(value).toLocaleString();
}

function formatNumber(value: number) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  return Intl.NumberFormat(undefined, { maximumFractionDigits: 8 }).format(value);
}

function formatOptionalNumber(value: number | null) {
  return value === null ? '-' : formatNumber(value);
}

function finiteNumber(value: number) {
  return Number.isFinite(value) ? value : 0;
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
