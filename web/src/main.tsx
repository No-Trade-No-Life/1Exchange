import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { HashRouter, Link, Navigate, NavLink, Route, Routes, useLocation, useSearchParams } from 'react-router-dom';
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

type AccountInfo = {
  account_id: string;
  positions: Position[];
};

type Position = {
  position_id: string;
  product_id: string;
  base_currency: string | null;
  quote_currency: string | null;
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
  accountId: string;
  credential: Credential;
  error: string | null;
  positions: Position[];
};

type TradeFill = {
  exchange: string;
  trade_id: string;
  order_id: string | null;
  product_id: string;
  direction: 'LONG' | 'SHORT' | null;
  price: number;
  volume: number;
  value: number;
  value_currency: string | null;
  fee: number;
  fee_currency: string | null;
  created_at: string | null;
};

type TradeAccount = {
  credential: Credential;
  error: string | null;
  trades: TradeFill[];
};

type AccountIds = Record<string, string>;

type Page = 'overview' | 'accounts' | 'portfolio' | 'trade' | 'history' | 'credentials' | 'positions' | 'products' | 'exchanges';

const pages: Array<{ id: Page; label: string; hint: string; path: string }> = [
  { id: 'overview', label: 'Overview', hint: 'Service and adapter status', path: '/overview' },
  { id: 'accounts', label: 'Accounts', hint: 'Account identities and detail', path: '/accounts' },
  { id: 'portfolio', label: 'Portfolio', hint: 'All credential assets', path: '/portfolio' },
  { id: 'trade', label: 'Trade', hint: 'Trading board', path: '/trade' },
  { id: 'history', label: 'History', hint: 'Trade fills', path: '/history' },
  { id: 'credentials', label: 'Credentials', hint: 'Saved local metadata', path: '/credentials' },
  { id: 'positions', label: 'Positions', hint: 'Assets and open exposure', path: '/positions' },
  { id: 'products', label: 'Products', hint: 'Exchange product specs', path: '/products' },
  { id: 'exchanges', label: 'Exchanges', hint: 'Schemas and capabilities', path: '/exchanges' },
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
          const response = await fetch(`/api/accounts?credential_id=${encodeURIComponent(credential.id)}`);
          if (!response.ok) {
            throw new Error(`${response.status} ${response.statusText}`);
          }
          const account = ((await response.json()) as AccountInfo[])[0];
          return {
            accountId: account?.account_id ?? fallbackAccountId(credential),
            credential,
            error: null,
            positions: account?.positions ?? [],
          };
        } catch (caught) {
          return {
            accountId: fallbackAccountId(credential),
            credential,
            error: (caught as Error).message,
            positions: [],
          };
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

function useTradeHistory(credentials: Credential[]) {
  const [accounts, setAccounts] = useState<TradeAccount[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let alive = true;
    setLoading(credentials.length > 0);
    setAccounts([]);

    Promise.all(
      credentials.map(async (credential) => {
        try {
          const response = await fetch(`/api/trades?credential_id=${encodeURIComponent(credential.id)}`);
          if (!response.ok) {
            throw new Error(`${response.status} ${response.statusText}`);
          }
          return { credential, error: null, trades: (await response.json()) as TradeFill[] };
        } catch (caught) {
          return { credential, error: (caught as Error).message, trades: [] };
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
  const location = useLocation();
  const [credentialsRevision, setCredentialsRevision] = useState(0);
  const health = useJson<Health>('/api/health');
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const credentials = useJson<Credential[]>(`/api/credentials?refresh=${credentialsRevision}`);
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
  const tradeHistory = useTradeHistory(credentialList);
  const accountIds = useMemo(
    () => Object.fromEntries(portfolio.accounts.map((account) => [account.credential.id, account.accountId])),
    [portfolio.accounts],
  );

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
  const page = currentPage(location.pathname);

  const overviewPage = (
      <OverviewPage
        credentialCount={credentials.data?.length ?? 0}
        database={health.data?.database ?? '~/.1ex/1ex.sqlite3'}
        exchangeCount={exchanges.data?.length ?? 0}
        healthStatus={health.data?.status ?? 'checking'}
        positionCount={positions.data?.length ?? 0}
        productCount={products.data?.length ?? 0}
        selectedCredential={selectedCredential}
      />
  );
  const accountsPage = <AccountsPage accounts={portfolio.accounts} loading={portfolio.loading} />;
  const accountDetailPage = <AccountDetailPage accounts={portfolio.accounts} loading={portfolio.loading} />;
  const portfolioPage = <PortfolioPage accounts={portfolio.accounts} loading={portfolio.loading} />;
  const historyPage = <TradeHistoryPage accountIds={accountIds} accounts={tradeHistory.accounts} loading={tradeHistory.loading} />;
  const tradePage = (
      <TradePage
        accountIds={accountIds}
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
  );
  const credentialsPage = (
    <CredentialsPage
      accountIds={accountIds}
      credentials={credentialList}
      exchanges={exchanges.data ?? []}
      onCreated={(credential) => {
        setSelectedCredentialId(credential.id);
        setCredentialsRevision((value) => value + 1);
      }}
    />
  );
  const positionsPage = (
      <PositionsPage
        accountIds={accountIds}
        credentials={credentialList}
        error={selectedCredentialId ? positions.error : null}
        exposure={exposure}
        loading={selectedCredentialId ? positions.loading : false}
        positions={selectedCredentialId ? positions.data ?? [] : []}
        selectedCredentialId={selectedCredentialId}
        onSelectCredential={setSelectedCredentialId}
      />
  );
  const productsPage = (
      <ProductsPage
        error={products.error}
        exchanges={exchanges.data ?? []}
        loading={products.loading}
        products={products.data ?? []}
        selectedExchangeId={selectedExchangeId}
        onSelectExchange={setSelectedExchangeId}
      />
  );
  const exchangesPage = <ExchangesPage exchanges={exchanges.data ?? []} />;

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
            <NavLink
              className={({ isActive }) => `nav-item${isActive ? ' active' : ''}`}
              key={item.id}
              to={item.path}
            >
              <span>{item.label}</span>
              <small>{item.hint}</small>
            </NavLink>
          ))}
        </nav>
      </aside>

      <main className="workspace">
        <header className="topbar">
          <div>
            <p className="section-label">{page.label}</p>
            <h1>{page.hint}</h1>
          </div>
          <div className="status-pill" data-state={health.data?.status === 'ok' ? 'ok' : 'pending'}>
            <span /> API {health.data?.status ?? 'checking'}
          </div>
        </header>
        <Routes>
          <Route path="/" element={<Navigate replace to="/overview" />} />
          <Route path="/overview" element={overviewPage} />
          <Route path="/accounts" element={accountsPage} />
          <Route path="/accounts/detail" element={accountDetailPage} />
          <Route path="/portfolio" element={portfolioPage} />
          <Route path="/trade" element={tradePage} />
          <Route path="/history" element={historyPage} />
          <Route path="/credentials" element={credentialsPage} />
          <Route path="/positions" element={positionsPage} />
          <Route path="/products" element={productsPage} />
          <Route path="/exchanges" element={exchangesPage} />
          <Route path="*" element={<Navigate replace to="/overview" />} />
        </Routes>
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
            <p className="section-label">By account</p>
            <h2>Account summary</h2>
          </div>
          <span className="count-chip">{props.loading ? 'Loading' : `${props.accounts.length} accounts`}</span>
        </div>
        <DataTable
          empty="No credentials yet. Add credentials before loading a portfolio summary."
          headers={['AccountID', 'Credential', 'Exchange', 'Positions', 'Assets', 'Long', 'Short', 'Notional value', 'Floating P/L', 'Status']}
          rows={props.accounts.map((account) => {
            const accountSummary = summarizeAccountPositions(account.positions);
            return [
              <AccountIdLink accountId={account.accountId} key="account" />,
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

function AccountsPage(props: { accounts: PortfolioAccount[]; loading: boolean }) {
  return (
    <div className="page-stack">
      <section className="metrics-grid compact" aria-label="Accounts summary">
        <Metric label="Accounts" value={props.accounts.length.toString()} />
        <Metric label="Loaded" value={props.accounts.filter((account) => !account.error).length.toString()} />
        <Metric label="Read errors" value={props.accounts.filter((account) => account.error).length.toString()} tone={props.accounts.some((account) => account.error) ? 'warn' : 'neutral'} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Account registry</p>
            <h2>Accounts</h2>
          </div>
          <span className="count-chip">{props.loading ? 'Loading' : `${props.accounts.length} accounts`}</span>
        </div>
        <DataTable
          empty="No accounts loaded yet. Add credentials first, then the account registry will appear here."
          headers={['AccountID', 'Credential', 'Exchange', 'Positions', 'Assets', 'Notional value', 'Floating P/L', 'Status']}
          rows={props.accounts.map((account) => {
            const summary = summarizeAccountPositions(account.positions);
            return [
              <AccountIdLink accountId={account.accountId} key="account" />,
              account.credential.name,
              <Badge key="exchange">{account.credential.exchange}</Badge>,
              summary.total.toString(),
              summary.assets.toString(),
              formatNotionalBreakdown(summary.notionalByCurrency),
              <Value key="pnl" value={summary.pnl} />,
              account.error ? <span className="status-text bad" key="status">{account.error}</span> : <span className="status-text good" key="status">Loaded</span>,
            ];
          })}
        />
      </section>
    </div>
  );
}

function AccountDetailPage(props: { accounts: PortfolioAccount[]; loading: boolean }) {
  const [params] = useSearchParams();
  const accountId = params.get('account_id') ?? '';
  const account = props.accounts.find((item) => item.accountId === accountId);
  const summary = summarizeAccountPositions(account?.positions ?? []);

  if (!accountId) {
    return <AccountDetailEmpty title="Select an account" message="Open the Accounts page and choose an AccountID to inspect positions and metadata." />;
  }

  if (!account) {
    return props.loading
      ? <AccountDetailEmpty title="Loading account" message="Account data is loading from saved credentials." />
      : <AccountDetailEmpty title="Account not found" message="This AccountID is not available in the current credential registry." />;
  }

  return (
    <div className="page-stack">
      <section className="panel account-detail-head">
        <div>
          <p className="section-label">Account detail</p>
          <h2><code>{account.accountId}</code></h2>
        </div>
        <Link className="secondary-link" to="/accounts">Back to accounts</Link>
      </section>

      <section className="metrics-grid compact" aria-label="Account detail summary">
        <Metric label="Positions" value={summary.total.toString()} />
        <Metric label="Assets" value={summary.assets.toString()} />
        <Metric label="Notional value" value={formatNotionalBreakdown(summary.notionalByCurrency)} />
        <Metric label="Floating P/L" value={formatNumber(summary.pnl)} tone={summary.pnl < 0 ? 'warn' : 'good'} />
      </section>

      <section className="panel">
        <div className="account-detail-grid">
          <DetailItem label="Credential" value={account.credential.name} />
          <DetailItem label="Exchange" value={account.credential.exchange} />
          <DetailItem label="Credential ID" value={account.credential.id} monospace />
          <DetailItem label="Payload" value={account.credential.has_payload ? 'Stored' : 'Missing'} />
          <DetailItem label="Created" value={formatDate(account.credential.created_at)} />
          <DetailItem label="Updated" value={formatDate(account.credential.updated_at)} />
        </div>
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Live read</p>
            <h2>Positions</h2>
          </div>
          <span className="count-chip">{account.positions.length} rows</span>
        </div>
        <InlineError message={account.error} />
        <DataTable
          empty="This account returned no positions."
          headers={['Position', 'Product', 'Base', 'Quote', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'Notional', 'P/L']}
          rows={account.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatPositionNotional(item),
            <Value key="pnl" value={item.floating_profit} />,
          ])}
        />
      </section>
    </div>
  );
}

function AccountDetailEmpty(props: { title: string; message: string }) {
  return (
    <section className="panel empty-detail">
      <p className="section-label">Account detail</p>
      <h2>{props.title}</h2>
      <p className="muted">{props.message}</p>
      <Link className="secondary-link" to="/accounts">Open Accounts</Link>
    </section>
  );
}

function DetailItem(props: { label: string; value: string; monospace?: boolean }) {
  return (
    <div className="detail-item">
      <span>{props.label}</span>
      {props.monospace ? <code>{props.value}</code> : <strong>{props.value}</strong>}
    </div>
  );
}

function AccountIdLink(props: { accountId: string }) {
  return (
    <Link className="account-id-link" to={accountDetailPath(props.accountId)}>
      <code>{props.accountId}</code>
    </Link>
  );
}

function TradeHistoryPage(props: { accountIds: AccountIds; accounts: TradeAccount[]; loading: boolean }) {
  const trades = props.accounts.flatMap((account) =>
    account.trades.map((trade) => ({ ...trade, accountId: accountIdForCredential(account.credential, props.accountIds) })),
  );
  const errors = props.accounts.filter((account) => account.error).length;

  return (
    <div className="page-stack">
      <section className="metrics-grid compact" aria-label="Trade history summary">
        <Metric label="Credentials" value={props.accounts.length.toString()} />
        <Metric label="Fills" value={trades.length.toString()} />
        <Metric label="Read errors" value={errors.toString()} tone={errors > 0 ? 'warn' : 'neutral'} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Recent fills</p>
            <h2>Historical trade flow</h2>
          </div>
          <span className="count-chip">{props.loading ? 'Loading' : `${trades.length} fills`}</span>
        </div>
        <DataTable
          empty="No trade fills returned. Some exchanges need trade-history permissions or a recent activity window."
          headers={['Time', 'AccountID', 'Exchange', 'Product', 'Side', 'Price', 'Volume', 'Value', 'Fee', 'Trade ID']}
          rows={trades
            .sort((a, b) => (b.created_at ?? '').localeCompare(a.created_at ?? ''))
            .slice(0, 500)
            .map((trade) => [
              formatTradeTime(trade.created_at),
              <AccountIdLink accountId={trade.accountId} key="account" />,
              <Badge key="exchange">{trade.exchange}</Badge>,
              <code key="product">{trade.product_id}</code>,
              trade.direction ? <Badge key="direction">{trade.direction}</Badge> : '-',
              formatNumber(trade.price),
              formatNumber(trade.volume),
              `${formatNumber(trade.value)} ${trade.value_currency ?? ''}`.trim(),
              `${formatNumber(trade.fee)} ${trade.fee_currency ?? ''}`.trim(),
              <code key="trade">{trade.trade_id}</code>,
            ])}
        />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">By account</p>
            <h2>History read status</h2>
          </div>
        </div>
        <DataTable
          empty="No credentials yet."
          headers={['AccountID', 'Credential', 'Exchange', 'Fills', 'Status']}
          rows={props.accounts.map((account) => [
            <AccountIdLink accountId={accountIdForCredential(account.credential, props.accountIds)} key="account" />,
            account.credential.name,
            <Badge key="exchange">{account.credential.exchange}</Badge>,
            account.trades.length.toString(),
            account.error ? <span className="status-text bad" key="status">{account.error}</span> : <span className="status-text good" key="status">Loaded</span>,
          ])}
        />
      </section>
    </div>
  );
}

function TradePage(props: {
  accountIds: AccountIds;
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
                {accountLabel(credential, props.accountIds)}
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
          headers={['Position', 'Base', 'Quote', 'Side', 'Volume', 'Entry', 'Mark', 'Notional', 'P/L']}
          rows={(relatedPositions.length ? relatedPositions : props.positions.slice(0, 8)).map((item) => [
            item.position_id,
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            item.direction ? <Badge key="side">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatPositionNotional(item),
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

function CredentialsPage(props: {
  accountIds: AccountIds;
  credentials: Credential[];
  exchanges: ExchangeInfo[];
  onCreated: (credential: Credential) => void;
}) {
  return (
    <div className="page-stack">
      <div className="credential-manager">
        <CredentialCreatePanel exchanges={props.exchanges} onCreated={props.onCreated} />
        <CredentialSecurityPanel />
      </div>

      <CredentialInventory accountIds={props.accountIds} credentials={props.credentials} />

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

function CredentialCreatePanel(props: { exchanges: ExchangeInfo[]; onCreated: (credential: Credential) => void }) {
  const [exchangeId, setExchangeId] = useState(props.exchanges[0]?.id ?? '');
  const [name, setName] = useState('');
  const [payload, setPayload] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [createdName, setCreatedName] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const selectedExchange = props.exchanges.find((exchange) => exchange.id === exchangeId) ?? props.exchanges[0];
  const fields = selectedExchange?.credential_schema.required ?? [];

  useEffect(() => {
    if (!exchangeId && props.exchanges[0]) {
      setExchangeId(props.exchanges[0].id);
    }
  }, [exchangeId, props.exchanges]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setCreatedName(null);
    setSaving(true);

    try {
      const response = await fetch('/api/credentials', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ exchange: exchangeId, name, payload: credentialPayload(fields, payload) }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? `${response.status} ${response.statusText}`);
      }
      const credential = await response.json() as Credential;
      setCreatedName(credential.name);
      setName('');
      setPayload({});
      props.onCreated(credential);
    } catch (caught) {
      setError((caught as Error).message);
    } finally {
      setSaving(false);
    }
  }

  return (
    <section className="panel credential-create-panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Add credential</p>
          <h2>Save read-only API access</h2>
        </div>
        <span className="count-chip">Local only</span>
      </div>
      <form className="credential-form" onSubmit={handleSubmit}>
        <label>
          Exchange
          <select value={exchangeId} onChange={(event) => { setExchangeId(event.target.value); setPayload({}); }}>
            {props.exchanges.map((exchange) => (
              <option key={exchange.id} value={exchange.id}>
                {exchange.id} · {exchange.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          Credential name
          <input required placeholder="readonly-main" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <div className="credential-field-grid">
          {fields.map((field) => (
            <label key={field}>
              {field}
              <input
                autoComplete="off"
                required
                type={isSecretCredentialField(field) ? 'password' : 'text'}
                value={payload[field] ?? ''}
                onChange={(event) => setPayload({ ...payload, [field]: event.target.value })}
              />
            </label>
          ))}
        </div>
        <InlineError message={error} />
        {createdName ? <p className="success-note">Saved {createdName}. Payload is stored server-side and hidden from this page.</p> : null}
        <button className="primary-action" disabled={saving || !exchangeId || !name} type="submit">
          {saving ? 'Saving...' : 'Save credential'}
        </button>
      </form>
    </section>
  );
}

function CredentialSecurityPanel() {
  return (
    <section className="panel credential-security-panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Handling rules</p>
          <h2>Payload stays hidden</h2>
        </div>
      </div>
      <div className="credential-rules">
        <p>Use exchange keys with read-only permissions. 1Exchange stores the payload locally, then only shows metadata and account identifiers in the GUI.</p>
        <dl>
          <div>
            <dt>Display identity</dt>
            <dd>AccountID uses exchange UID when the adapter can read it.</dd>
          </div>
          <div>
            <dt>Payload visibility</dt>
            <dd>Secret fields are accepted once and never rendered back.</dd>
          </div>
          <div>
            <dt>Next check</dt>
            <dd>Open Portfolio or Positions to validate that the key can read live data.</dd>
          </div>
        </dl>
      </div>
    </section>
  );
}

function CredentialInventory(props: { accountIds: AccountIds; credentials: Credential[] }) {
  return (
    <section className="panel">
      <div className="panel-heading">
        <div>
          <p className="section-label">Saved locally</p>
          <h2>Credential inventory</h2>
        </div>
        <span className="count-chip">{props.credentials.length} saved</span>
      </div>
      <DataTable
        empty="No credentials yet. Add a read-only credential from the form above."
        headers={['AccountID', 'Name', 'Exchange', 'Payload', 'Created', 'Credential ID']}
        rows={props.credentials.map((item) => [
          <AccountIdLink accountId={accountIdForCredential(item, props.accountIds)} key="account" />,
          item.name,
          <Badge key="exchange">{item.exchange}</Badge>,
          item.has_payload ? 'Stored' : 'Missing',
          formatDate(item.created_at),
          <code key="id">{item.id}</code>,
        ])}
      />
    </section>
  );
}

function PositionsPage(props: {
  accountIds: AccountIds;
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
                {accountLabel(credential, props.accountIds)}
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
          headers={['Position', 'Product', 'Base', 'Quote', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'Notional', 'P/L']}
          rows={props.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatPositionNotional(item),
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

function currentPage(pathname: string) {
  return pages.find((item) => item.path === pathname) ?? pages.find((item) => pathname.startsWith(`${item.path}/`)) ?? pages[0];
}

function accountIdForCredential(credential: Credential, accountIds: AccountIds) {
  return accountIds[credential.id] ?? fallbackAccountId(credential);
}

function accountLabel(credential: Credential, accountIds: AccountIds) {
  const accountId = accountIds[credential.id];
  return accountId ? `${accountId} · ${credential.name}` : fallbackAccountId(credential);
}

function accountDetailPath(accountId: string) {
  return `/accounts/detail?account_id=${encodeURIComponent(accountId)}`;
}

function fallbackAccountId(credential: Credential) {
  return `${credential.exchange}/local:${credential.id.slice(0, 8)}`;
}

function credentialPayload(fields: string[], values: Record<string, string>) {
  return Object.fromEntries(fields.map((field) => [field, values[field] ?? '']));
}

function isSecretCredentialField(field: string) {
  return ['secret', 'key', 'passphrase', 'private', 'signer'].some((part) => field.includes(part));
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

function formatPositionNotional(position: Position) {
  return `${formatNumber(notionalValue(position))} ${position.notional_currency ?? position.quote_currency ?? ''}`.trim();
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

function formatTradeTime(value: string | null) {
  if (!value) {
    return '-';
  }
  const numeric = Number(value);
  if (Number.isFinite(numeric)) {
    const ms = numeric > 10_000_000_000 ? numeric : numeric * 1000;
    return new Date(ms).toLocaleString();
  }
  return formatDate(value);
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
    <HashRouter>
      <App />
    </HashRouter>
  </React.StrictMode>,
);
