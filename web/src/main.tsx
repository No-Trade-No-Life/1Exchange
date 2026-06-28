import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { QueryClient, QueryClientProvider, useQuery, useQueryClient } from '@tanstack/react-query';
import { createRoot } from 'react-dom/client';
import { ErrorBoundary } from 'react-error-boundary';
import { HashRouter, Link, Navigate, NavLink, Route, Routes, useLocation, useSearchParams } from 'react-router-dom';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Badge as UiBadge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Checkbox } from '@/components/ui/checkbox';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { cn } from '@/lib/utils';
import {
  BadgeCheck,
  BarChart3,
  ChevronDown,
  History,
  LayoutDashboard,
  LineChart,
  Menu,
  PackageSearch,
  WalletCards,
  type LucideIcon,
} from 'lucide-react';
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

type AccountRef = {
  credential_id: string;
  account_id: string | null;
  error: string | null;
};

type Position = {
  position_id: string;
  product_id: string;
  base_currency: string | null;
  quote_currency: string | null;
  direction: 'LONG' | 'SHORT' | null;
  datasource_id?: string | null;
  account_id?: string | null;
  size?: string | null;
  free_size?: string | null;
  volume: number;
  free_volume: number;
  liquidation_price?: string | null;
  position_price: number;
  closable_price: number;
  current_price?: string | null;
  notional_value: number;
  notional_currency: string | null;
  notional?: string | null;
  valuation?: number;
  floating_profit: number;
  comment: string | null;
  settlement_interval?: number | null;
  settlement_scheduled_at?: number | null;
  interest_to_settle?: number | null;
  margin?: number | null;
  realized_pnl?: number | null;
  total_opened_volume?: number | null;
  total_closed_volume?: number | null;
  created_at?: number | null;
  updated_at?: number | null;
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
  market_id?: string | null;
  no_interest_rate?: boolean | null;
};

type CurrencyRateEdge = {
  base_currency: string;
  quote_currency: string;
  rate: number;
  source: string;
  updated_at: string;
};

type CurrencyRateSnapshot = {
  target_currency: string;
  edges: CurrencyRateEdge[];
};

type AccountSnapshot = {
  accountId: string;
  credential: Credential;
  error: string | null;
  positions: Position[];
  sourceLabel: string;
  sourceType: AccountSourceType;
};

type AccountSourceType = 'credential' | 'virtual' | 'custom';

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

type VirtualAccountSource = {
  credential_id: string;
  coefficient: number;
  enabled: boolean;
  force_zero: boolean;
};

type VirtualAccountConfig = {
  account_id: string;
  name: string;
  enabled: boolean;
  sources: VirtualAccountSource[];
  created_at: string;
  updated_at: string;
};

type FundConfig = {
  id: string;
  name: string;
  account_id: string;
  enabled: boolean;
  target_currency: string;
  poll_interval_seconds: number;
  created_at: string;
  updated_at: string;
  last_sampled_at: string | null;
};

type FundNavSnapshot = {
  id: string;
  fund_id: string;
  account_id: string;
  equity: number;
  target_currency: string;
  positions_count: number;
  unpriced_positions: number;
  created_at: string;
};

type CustomAccountSource = {
  id: string;
  name: string;
  base_url: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
};

type AccountIds = Record<string, string>;
type JsonResource = {
  error: string | null;
  loading: boolean;
  refresh: () => Promise<unknown>;
  refreshing: boolean;
};

type BatchLoadState = {
  loaded: number;
  loading: boolean;
  total: number;
};

type Page = 'overview' | 'accounts' | 'history' | 'funds' | 'positions' | 'products' | 'exchanges';
type PageConfig = { id: Page; label: string; hint: string; path: string; icon: LucideIcon; primary?: boolean };

const pages: PageConfig[] = [
  { id: 'overview', label: 'Overview', hint: 'Service and adapter status', path: '/overview', icon: LayoutDashboard, primary: true },
  { id: 'accounts', label: 'Accounts', hint: 'Account identities and detail', path: '/accounts', icon: WalletCards, primary: true },
  { id: 'history', label: 'Audit', hint: 'Read-only fill history', path: '/history', icon: History },
  { id: 'funds', label: 'Funds', hint: 'Virtual account NAV records', path: '/funds', icon: LineChart, primary: true },
  { id: 'positions', label: 'Positions', hint: 'Assets and open exposure', path: '/positions', icon: BarChart3, primary: true },
  { id: 'products', label: 'Products', hint: 'Exchange product specs', path: '/products', icon: PackageSearch },
  { id: 'exchanges', label: 'Exchanges', hint: 'Schemas and capabilities', path: '/exchanges', icon: BadgeCheck },
];
const primaryPages = pages.filter((item) => item.primary);

const emptyCredentials: Credential[] = [];
const emptyVirtualAccountConfigs: VirtualAccountConfig[] = [];
const emptyFundConfigs: FundConfig[] = [];
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      refetchOnWindowFocus: false,
      staleTime: 30_000,
    },
  },
});

function useJson<T>(path: string | null) {
  const query = useQuery({
    enabled: path !== null,
    queryKey: ['json', path],
    queryFn: async ({ queryKey }) => {
      const requestPath = queryKey[1] as string;
      const response = await fetch(requestPath);
      if (!response.ok) {
        throw new Error(`${response.status} ${response.statusText}`);
      }
      return response.json() as Promise<T>;
    },
  });

  const refresh = useCallback(() => {
    if (path === null) {
      return Promise.resolve(null);
    }
    return query.refetch();
  }, [path, query.refetch]);

  return {
    data: query.data ?? null,
    error: query.error?.message ?? null,
    loading: query.isLoading,
    refresh,
    refreshing: query.isFetching && !query.isLoading,
  };
}

function useTradeHistory(credentials: Credential[], refreshToken: number) {
  const [accounts, setAccounts] = useState<TradeAccount[]>([]);
  const [loadState, setLoadState] = useState<BatchLoadState>({ loaded: 0, loading: false, total: 0 });

  useEffect(() => {
    let alive = true;
    setLoadState({ loaded: 0, loading: credentials.length > 0, total: credentials.length });
    setAccounts([]);

    async function readCredential(credential: Credential) {
        try {
          const response = await fetch(`/api/trades?credential_id=${encodeURIComponent(credential.id)}`);
          if (!response.ok) {
            throw new Error(`${response.status} ${response.statusText}`);
          }
          return { credential, error: null, trades: (await response.json()) as TradeFill[] };
        } catch (caught) {
          return { credential, error: (caught as Error).message, trades: [] };
        }
    }

    credentials.forEach((credential) => {
      void readCredential(credential).then((account) => {
        if (!alive) {
          return;
        }
        setAccounts((current) => [...current, account]);
        setLoadState((current) => {
          const loaded = current.loaded + 1;
          return { ...current, loaded, loading: loaded < current.total };
        });
      });
    });

    return () => {
      alive = false;
    };
  }, [credentials, refreshToken]);

  return { accounts, ...loadState };
}

function App() {
  const location = useLocation();
  const page = currentPage(location.pathname);
  const health = useJson<Health>('/api/health');

  return (
    <div className="min-h-screen bg-muted/30">
      <AppHeader healthStatus={health.data?.status ?? 'checking'} page={page} />

      <main className="mx-auto w-full max-w-[1600px] px-7 py-6 max-md:px-4">
        <Routes>
          <Route path="/" element={<Navigate replace to="/overview" />} />
          <Route path="/overview" element={<PageBoundary><OverviewRoute /></PageBoundary>} />
          <Route path="/accounts" element={<PageBoundary><AccountsRoute /></PageBoundary>} />
          <Route path="/accounts/detail" element={<PageBoundary><AccountDetailRoute /></PageBoundary>} />
          <Route path="/history" element={<PageBoundary><TradeHistoryRoute /></PageBoundary>} />
          <Route path="/credentials" element={<Navigate replace to="/accounts" />} />
          <Route path="/virtual-accounts" element={<Navigate replace to="/accounts" />} />
          <Route path="/funds" element={<PageBoundary><FundsRoute /></PageBoundary>} />
          <Route path="/positions" element={<PageBoundary><PositionsRoute /></PageBoundary>} />
          <Route path="/products" element={<PageBoundary><ProductsRoute /></PageBoundary>} />
          <Route path="/exchanges" element={<PageBoundary><ExchangesRoute /></PageBoundary>} />
          <Route path="*" element={<Navigate replace to="/overview" />} />
        </Routes>
      </main>
    </div>
  );
}

function AppHeader(props: { healthStatus: string; page: PageConfig }) {
  const CurrentIcon = props.page.icon;

  return (
    <header className="sticky top-0 z-40 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/80">
      <div className="mx-auto flex w-full max-w-[1600px] items-center gap-5 px-7 py-3 max-md:px-4">
        <Link className="flex min-w-0 items-center gap-3 rounded-lg focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50" to="/overview">
          <span className="grid size-9 place-items-center rounded-lg bg-primary text-sm font-semibold text-primary-foreground">1E</span>
          <span className="min-w-0">
            <strong className="block leading-tight">1Exchange</strong>
            <span className="block truncate text-xs text-muted-foreground">Local account console</span>
          </span>
        </Link>

        <nav className="hidden items-center gap-1 lg:flex" aria-label="Primary navigation">
          {primaryPages.map((item) => (
            <TopNavLink item={item} key={item.id} />
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-2">
          <UiBadge className="hidden sm:inline-flex" variant={props.healthStatus === 'ok' ? 'secondary' : 'outline'}>API {props.healthStatus}</UiBadge>
          <DropdownNavigation currentPage={props.page} />
        </div>
      </div>

      <div className="mx-auto flex w-full max-w-[1600px] items-center gap-2 px-7 pb-4 max-md:px-4">
        <span className="text-muted-foreground">
          <CurrentIcon />
        </span>
        <div className="min-w-0">
          <SectionLabel>{props.page.label}</SectionLabel>
          <h1 className="truncate text-2xl font-semibold leading-tight">{props.page.hint}</h1>
        </div>
      </div>
    </header>
  );
}

function TopNavLink(props: { item: PageConfig }) {
  const Icon = props.item.icon;

  return (
    <NavLink
      className={({ isActive }) =>
        cn(
          'inline-flex h-8 items-center gap-1.5 rounded-lg px-2.5 text-sm font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
          isActive && 'bg-muted text-foreground',
        )
      }
      to={props.item.path}
    >
      <Icon />
      {props.item.label}
    </NavLink>
  );
}

function DropdownNavigation(props: { currentPage: PageConfig }) {
  const CurrentIcon = props.currentPage.icon;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger render={
        <Button variant="outline">
          <Menu data-icon="inline-start" />
          <span className="hidden sm:inline">Menu</span>
          <ChevronDown data-icon="inline-end" />
        </Button>
      } />
      <DropdownMenuContent align="end" className="w-72">
        <DropdownMenuGroup>
          <DropdownMenuLabel>Navigation</DropdownMenuLabel>
          {pages.map((item) => {
            const Icon = item.icon;
            return (
              <DropdownMenuItem
                className={cn(item.id === props.currentPage.id && 'bg-muted text-foreground')}
                key={item.id}
                render={<Link to={item.path} />}
              >
                <Icon />
                <span className="flex min-w-0 flex-col">
                  <span className="font-medium">{item.label}</span>
                  <span className="truncate text-xs text-muted-foreground">{item.hint}</span>
                </span>
              </DropdownMenuItem>
            );
          })}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled>
          <CurrentIcon />
          <span>Current: {props.currentPage.label}</span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

function PageBoundary(props: { children: React.ReactNode }) {
  return <ErrorBoundary FallbackComponent={PageErrorFallback}>{props.children}</ErrorBoundary>;
}

function PageErrorFallback(props: { error: unknown; resetErrorBoundary: () => void }) {
  const message = props.error instanceof Error ? props.error.message : String(props.error);

  return (
    <Card>
      <CardHeader>
        <SectionLabel>Page error</SectionLabel>
        <CardTitle>Something went wrong</CardTitle>
        <CardDescription>{message}</CardDescription>
      </CardHeader>
      <CardContent>
        <Button variant="outline" type="button" onClick={props.resetErrorBoundary}>Try again</Button>
      </CardContent>
    </Card>
  );
}

function RefreshScope(props: {
  batch?: BatchLoadState & { refresh: () => void };
  children: React.ReactNode;
  resources: JsonResource[];
}) {
  const queryClient = useQueryClient();
  const loading = props.resources.some((resource) => resource.loading) || Boolean(props.batch?.loading);
  const refreshing = props.resources.some((resource) => resource.refreshing);
  const status = refreshStatusLabel(loading, refreshing, props.batch);

  async function refreshPage() {
    props.batch?.refresh();
    await Promise.all(props.resources.map((resource) => resource.refresh()));
  }

  async function refreshAll() {
    props.batch?.refresh();
    await queryClient.invalidateQueries({ queryKey: ['json'] });
  }

  return (
    <div className="page-stack">
      <Card className="py-3" aria-label="Refresh controls">
        <CardContent className="flex items-center justify-between gap-3 max-md:flex-col max-md:items-stretch">
        <LoadingStatus active={loading || refreshing} label={status} />
        <div className="flex flex-wrap justify-end gap-2">
          <Button variant="outline" disabled={loading || refreshing} type="button" onClick={() => void refreshPage()}>
            Refresh page
          </Button>
          <Button variant="outline" disabled={loading || refreshing} type="button" onClick={() => void refreshAll()}>
            Refresh all
          </Button>
        </div>
        </CardContent>
      </Card>
      {props.children}
    </div>
  );
}

function LoadingStatus(props: { active: boolean; label: string }) {
  return (
    <span className={cn('inline-flex w-fit items-center gap-2 text-xs font-medium text-muted-foreground', props.active && 'text-foreground')}>
      {props.active ? <span className="size-3 animate-spin rounded-full border-2 border-muted border-t-primary" aria-hidden="true" /> : <span className="size-3 rounded-full bg-primary" aria-hidden="true" />}
      {props.label}
    </span>
  );
}

function refreshStatusLabel(loading: boolean, refreshing: boolean, batch?: BatchLoadState) {
  if (batch?.loading) {
    return 'Loading ' + batch.loaded + '/' + batch.total;
  }
  if (loading) {
    return 'Loading data';
  }
  if (refreshing) {
    return 'Refreshing';
  }
  return 'Ready';
}

function OverviewRoute() {
  const health = useJson<Health>('/api/health');
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const credentials = useJson<Credential[]>('/api/credentials');
  const firstCredential = credentials.data?.[0];

  return (
    <RefreshScope resources={[health, exchanges, credentials]}>
      <OverviewPage
        credentialCount={credentials.data?.length ?? 0}
        database={health.data?.database ?? '~/.1ex/1ex.sqlite3'}
        exchangeCount={exchanges.data?.length ?? 0}
        healthStatus={health.data?.status ?? 'checking'}
        selectedCredential={firstCredential}
      />
    </RefreshScope>
  );
}

function AccountsRoute() {
  const queryClient = useQueryClient();
  const rates = useJson<CurrencyRateSnapshot>('/api/rates?target=USD');
  const customAccountSources = useJson<CustomAccountSource[]>('/api/custom-account-sources');
  const credentials = useJson<Credential[]>('/api/credentials');
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const accountRefs = useJson<AccountRef[]>('/api/account-refs');
  const virtualAccounts = useJson<VirtualAccountConfig[]>('/api/virtual-accounts');
  const credentialList = credentials.data ?? emptyCredentials;
  const accountIds = useMemo(
    () => accountIdsFromRefs(accountRefs.data ?? []),
    [accountRefs.data],
  );
  const accounts = [
    ...credentialAccounts(credentialList, accountRefs.data ?? []),
    ...virtualAccountSnapshots(virtualAccounts.data ?? emptyVirtualAccountConfigs),
  ];

  return (
    <RefreshScope resources={[rates, customAccountSources, credentials, exchanges, accountRefs, virtualAccounts]}>
      <AccountsPage
        accountIds={accountIds}
        accounts={accounts}
        credentials={credentialList}
        customSources={customAccountSources.data ?? []}
        exchanges={exchanges.data ?? []}
        loading={credentials.loading || accountRefs.loading || customAccountSources.loading || exchanges.loading || virtualAccounts.loading}
        rateEdges={rates.data?.edges ?? []}
        virtualAccounts={virtualAccounts.data ?? emptyVirtualAccountConfigs}
        onCredentialCreated={() => {
          void queryClient.invalidateQueries({ queryKey: ['json', '/api/credentials'] });
          void queryClient.invalidateQueries({ queryKey: ['json', '/api/account-refs'] });
        }}
      />
    </RefreshScope>
  );
}

function AccountDetailRoute() {
  const [params] = useSearchParams();
  const accountId = params.get('account_id') ?? '';
  const rates = useJson<CurrencyRateSnapshot>('/api/rates?target=USD');
  const account = useJson<AccountInfo[]>(accountId ? `/api/accounts?account_id=${encodeURIComponent(accountId)}` : null);
  const accounts = (account.data ?? []).map(accountInfoToAccountSnapshot);

  return (
    <RefreshScope resources={[rates, account]}>
      <AccountDetailPage accounts={accounts} loading={account.loading} rateEdges={rates.data?.edges ?? []} />
    </RefreshScope>
  );
}

function TradeHistoryRoute() {
  const credentials = useJson<Credential[]>('/api/credentials');
  const accountRefs = useJson<AccountRef[]>('/api/account-refs');
  const credentialList = credentials.data ?? emptyCredentials;
  const [refreshToken, setRefreshToken] = useState(0);
  const tradeHistory = useTradeHistory(credentialList, refreshToken);
  const batch = { ...tradeHistory, refresh: () => setRefreshToken((value) => value + 1) };
  const accountIds = useMemo(
    () => accountIdsFromRefs(accountRefs.data ?? []),
    [accountRefs.data],
  );

  return (
    <RefreshScope batch={batch} resources={[credentials, accountRefs]}>
      <TradeHistoryPage accountIds={accountIds} accounts={tradeHistory.accounts} loading={tradeHistory.loading} loadState={tradeHistory} />
    </RefreshScope>
  );
}

function FundsRoute() {
  const funds = useJson<FundConfig[]>('/api/funds');
  const virtualAccounts = useJson<VirtualAccountConfig[]>('/api/virtual-accounts');
  const selectedFundId = funds.data?.[0]?.id ?? '';
  const nav = useJson<FundNavSnapshot[]>(selectedFundId ? `/api/fund-nav?fund_id=${encodeURIComponent(selectedFundId)}&limit=25` : null);

  return (
    <RefreshScope resources={[funds, virtualAccounts, nav]}>
      <FundsPage
        configs={funds.data ?? emptyFundConfigs}
        error={funds.error ?? virtualAccounts.error ?? nav.error}
        loading={funds.loading || virtualAccounts.loading || nav.loading}
        snapshots={nav.data ?? []}
        virtualAccounts={virtualAccounts.data ?? emptyVirtualAccountConfigs}
      />
    </RefreshScope>
  );
}

function PositionsRoute() {
  const credentials = useJson<Credential[]>('/api/credentials');
  const accountRefs = useJson<AccountRef[]>('/api/account-refs');
  const credentialList = credentials.data ?? emptyCredentials;
  const accountIds = useMemo(
    () => accountIdsFromRefs(accountRefs.data ?? []),
    [accountRefs.data],
  );
  const [selectedCredentialId, setSelectedCredentialId] = useState('');
  const positions = useJson<Position[]>(selectedCredentialId ? `/api/positions?credential_id=${encodeURIComponent(selectedCredentialId)}` : null);

  useEffect(() => {
    if (!selectedCredentialId && credentialList[0]) {
      setSelectedCredentialId(credentialList[0].id);
    }
  }, [credentialList, selectedCredentialId]);

  return (
    <RefreshScope resources={[credentials, accountRefs, positions]}>
      <PositionsPage
        accountIds={accountIds}
        credentials={credentialList}
        error={selectedCredentialId ? positions.error : null}
        exposure={summarizePositions(positions.data ?? [])}
        loading={selectedCredentialId ? positions.loading : false}
        positions={selectedCredentialId ? positions.data ?? [] : []}
        selectedCredentialId={selectedCredentialId}
        onSelectCredential={setSelectedCredentialId}
      />
    </RefreshScope>
  );
}

function ProductsRoute() {
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');
  const [selectedExchangeId, setSelectedExchangeId] = useState('BINANCE');
  const products = useJson<Product[]>(`/api/products?exchange=${encodeURIComponent(selectedExchangeId)}`);

  return (
    <RefreshScope resources={[exchanges, products]}>
      <ProductsPage
        error={products.error}
        exchanges={exchanges.data ?? []}
        loading={products.loading}
        products={products.data ?? []}
        selectedExchangeId={selectedExchangeId}
        onSelectExchange={setSelectedExchangeId}
      />
    </RefreshScope>
  );
}

function ExchangesRoute() {
  const exchanges = useJson<ExchangeInfo[]>('/api/exchanges');

  return (
    <RefreshScope resources={[exchanges]}>
      <ExchangesPage exchanges={exchanges.data ?? []} />
    </RefreshScope>
  );
}

function OverviewPage(props: {
  credentialCount: number;
  database: string;
  exchangeCount: number;
  healthStatus: string;
  selectedCredential?: Credential;
}) {
  return (
    <div className="page-stack">
      <section className="metrics-grid" aria-label="System summary">
        <Metric label="API" value={props.healthStatus} tone={props.healthStatus === 'ok' ? 'good' : 'neutral'} />
        <Metric label="Exchanges" value={props.exchangeCount.toString()} />
        <Metric label="Credentials" value={props.credentialCount.toString()} />
        <Metric label="Live reads" value="On demand" />
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
            <dd>On demand</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}

function AccountsPage(props: {
  accountIds: AccountIds;
  accounts: AccountSnapshot[];
  credentials: Credential[];
  customSources: CustomAccountSource[];
  exchanges: ExchangeInfo[];
  loading: boolean;
  rateEdges: CurrencyRateEdge[];
  virtualAccounts: VirtualAccountConfig[];
  onCredentialCreated: (credential: Credential) => void;
}) {
  return (
    <div className="page-stack">
      <section className="metrics-grid compact" aria-label="Accounts summary">
        <Metric label="Accounts" value={props.accounts.length.toString()} />
        <Metric label="EX credentials" value={props.credentials.length.toString()} />
        <Metric label="Virtual accounts" value={props.virtualAccounts.length.toString()} />
        <Metric label="Custom sources" value={props.customSources.length.toString()} />
        <Metric label="Loaded" value={props.accounts.filter((account) => !account.error).length.toString()} />
        <Metric label="Read errors" value={props.accounts.filter((account) => account.error).length.toString()} tone={props.accounts.some((account) => account.error) ? 'warn' : 'neutral'} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel" id="account-registry">
        <div className="panel-heading">
          <div>
            <p className="section-label">Account registry</p>
            <h2>Accounts</h2>
          </div>
          <LoadingStatus active={props.loading} label={props.loading ? 'Loading' : `${props.accounts.length} accounts`} />
        </div>
        <DataTable
          empty="No accounts loaded yet. Add a credential, virtual account, or custom account source below."
          headers={['AccountID', 'Source type', 'Name', 'Protocol', 'Positions', 'Assets', 'USD converted', 'Floating P/L', 'Status', 'Action']}
          rows={props.accounts.map((account) => {
            const summary = summarizeAccountPositions(account.positions);
            return [
              <AccountIdLink accountId={account.accountId} key="account" />,
              sourceTypeLabel(account.sourceType),
              account.credential.name,
              <Badge key="protocol">{account.sourceLabel}</Badge>,
              summary.total.toString(),
              summary.assets.toString(),
              formatConvertedValue(convertCurrencyTotals(summary.notionalByCurrency, 'USD', props.rateEdges), 'USD'),
              <Value key="pnl" value={summary.pnl} />,
              account.error ? <span className="status-text bad" key="status">{account.error}</span> : <span className="status-text good" key="status">Loaded</span>,
              <Button key="action" variant="outline" type="button" onClick={() => scrollToAccountSource(account.sourceType)}>
                Edit
              </Button>,
            ];
          })}
        />
      </section>

      <div className="account-source-section" id="account-source-credential">
        <PanelTitle label="Account source" title="Real EX credentials" action={props.credentials.length + ' saved'} />
        <div className="credential-manager">
          <CredentialCreatePanel exchanges={props.exchanges} onCreated={props.onCredentialCreated} />
          <CredentialSecurityPanel />
        </div>
        <CredentialInventory accountIds={props.accountIds} credentials={props.credentials} />
        <CredentialSchemaPanel exchanges={props.exchanges} />
      </div>

      <div className="account-source-section" id="account-source-virtual">
        <PanelTitle label="Account source" title="Virtual accounts" action={props.virtualAccounts.length + ' configs'} />
        <VirtualAccountCreatePanel accountIds={props.accountIds} credentials={props.credentials} />
        <VirtualAccountInventory configs={props.virtualAccounts} />
      </div>

      <div className="account-source-section" id="account-source-custom">
        <PanelTitle label="Account source" title="Custom account sources" action={props.customSources.length + ' sources'} />
        <CustomAccountSourcePanel sources={props.customSources} />
      </div>
    </div>
  );
}

function CustomAccountSourcePanel(props: { sources: CustomAccountSource[] }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState('Remote 1Exchange');
  const [baseUrl, setBaseUrl] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setSaving(true);

    try {
      const response = await fetch('/api/custom-account-sources', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ name, base_url: baseUrl, enabled: true }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? `${response.status} ${response.statusText}`);
      }
      await queryClient.invalidateQueries({ queryKey: ['json', '/api/custom-account-sources'] });
      await queryClient.invalidateQueries({ queryKey: ['json', '/api/accounts'] });
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
          <p className="section-label">Custom account source</p>
          <h2>Register a 1Exchange-compatible BASE URL</h2>
        </div>
        <span className="count-chip">{props.sources.length} sources</span>
      </div>
      <form className="credential-form" onSubmit={handleSubmit}>
        <label>
          Source name
          <Input required value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label>
          BASE URL
          <Input required placeholder="http://127.0.0.1:8788" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </label>
        <p className="form-note">The remote server must expose the 1Exchange account API subset: GET /api/accounts and GET /api/accounts?account_id=...</p>
        <InlineError message={error} />
        <Button disabled={saving || !name || !baseUrl} type="submit">
          {saving ? 'Saving...' : 'Register source'}
        </Button>
      </form>
      <DataTable
        empty="No custom account sources registered."
        headers={['Name', 'BASE URL', 'Status', 'Updated']}
        rows={props.sources.map((source) => [
          source.name,
          <code key="url">{source.base_url}</code>,
          source.enabled ? 'Enabled' : 'Disabled',
          source.updated_at,
        ])}
      />
    </section>
  );
}

function AccountDetailPage(props: { accounts: AccountSnapshot[]; loading: boolean; rateEdges: CurrencyRateEdge[] }) {
  const [params] = useSearchParams();
  const accountId = params.get('account_id') ?? '';
  const account = props.accounts.find((item) => item.accountId === accountId);
  const positions = account?.positions ?? [];
  const summary = summarizeAccountPositions(positions);
  const assetRows = summarizeAccountAssets(positions);
  const usdValue = convertCurrencyTotals(summary.notionalByCurrency, 'USD', props.rateEdges);

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
        <Metric label="USD converted" value={formatConvertedValue(usdValue, 'USD')} tone={usdValue.unconverted.length > 0 ? 'warn' : 'neutral'} />
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
            <p className="section-label">Account portfolio</p>
            <h2>Asset exposure</h2>
          </div>
          <span className="count-chip">{assetRows.length} products</span>
        </div>
        <InlineError message={account.error} />
        <DataTable
          empty="This account returned no asset exposure."
          headers={['Product', 'Currency', 'Rows', 'Volume', 'Free', 'Notional value', 'Floating P/L']}
          rows={assetRows.map((row) => [
            <code key="product">{row.productId}</code>,
            row.currency,
            row.rows.toString(),
            formatNumber(row.volume),
            formatNumber(row.freeVolume),
            formatNumber(row.notionalValue),
            <Value key="pnl" value={row.pnl} />,
          ])}
        />
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

function TradeHistoryPage(props: { accountIds: AccountIds; accounts: TradeAccount[]; loading: boolean; loadState: BatchLoadState }) {
  const trades = props.accounts.flatMap((account) =>
    account.trades.map((trade) => ({ ...trade, accountId: accountIdForCredential(account.credential, props.accountIds) })),
  );
  const errors = props.accounts.filter((account) => account.error).length;

  return (
    <div className="page-stack">
      <section className="metrics-grid compact" aria-label="Read-only audit summary">
        <Metric label="Credentials" value={props.accounts.length.toString()} />
        <Metric label="Fills" value={trades.length.toString()} />
        <Metric label="Read errors" value={errors.toString()} tone={errors > 0 ? 'warn' : 'neutral'} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">Read-only audit</p>
            <h2>Historical fills</h2>
          </div>
          <LoadingStatus
            active={props.loading}
            label={props.loading ? `Loading ${props.loadState.loaded}/${props.loadState.total}` : `${trades.length} fills`}
          />
        </div>
        <DataTable
          empty="No fills returned. Some exchanges need read-only history permissions or a recent activity window."
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
            <h2>Audit read status</h2>
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

function PanelTitle(props: { action?: string; label: string; title: string }) {
  return (
    <div className="panel-title">
      <div>
        <p className="section-label">{props.label}</p>
        <h2>{props.title}</h2>
      </div>
      {props.action ? <span className="count-chip">{props.action}</span> : null}
    </div>
  );
}

function CredentialSchemaPanel(props: { exchanges: ExchangeInfo[] }) {
  return (
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
  );
}

function VirtualAccountInventory(props: { configs: VirtualAccountConfig[] }) {
  return (
    <section className="panel">
      <PanelTitle label="Virtual account configs" title="Local linear compositions" action={props.configs.length + ' configs'} />
      <DataTable
        empty="No virtual account configs yet."
        headers={['Account', 'Name', 'Status', 'Sources', 'Updated', 'Action']}
        rows={props.configs.map((config) => [
          <code key="account">{config.account_id}</code>,
          config.name,
          config.enabled ? 'Enabled' : 'Disabled',
          config.sources.filter((source) => source.enabled).length + '/' + config.sources.length,
          config.updated_at,
          config.enabled ? <AccountIdLink accountId={config.account_id} key="action" /> : '-',
        ])}
      />
    </section>
  );
}

function VirtualAccountCreatePanel(props: { accountIds: AccountIds; credentials: Credential[] }) {
  const queryClient = useQueryClient();
  const [accountId, setAccountId] = useState('VIRTUAL/local');
  const [name, setName] = useState('Local virtual account');
  const [sources, setSources] = useState<VirtualAccountSource[]>([
    { credential_id: props.credentials[0]?.id ?? '', coefficient: 1, enabled: true, force_zero: false },
  ]);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (sources.length === 1 && !sources[0].credential_id && props.credentials[0]) {
      setSources([{ ...sources[0], credential_id: props.credentials[0].id }]);
    }
  }, [props.credentials, sources]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setSaving(true);

    try {
      const response = await fetch('/api/virtual-accounts', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ account_id: accountId, name, enabled: true, sources }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? `${response.status} ${response.statusText}`);
      }
      await queryClient.invalidateQueries({ queryKey: ['json', '/api/virtual-accounts'] });
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
          <p className="section-label">Create virtual account</p>
          <h2>Linear source composition</h2>
        </div>
        <span className="count-chip">On demand</span>
      </div>
      <form className="credential-form" onSubmit={handleSubmit}>
        <label>
          Virtual account ID
          <Input required value={accountId} onChange={(event) => setAccountId(event.target.value)} />
        </label>
        <label>
          Name
          <Input required value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <p className="form-note">Coefficient expresses add/subtract/multiply/divide: 1 adds, -1 subtracts, 2 multiplies, 0.5 divides by 2. The account is composed only when queried.</p>
        <div className="schema-grid">
          {sources.map((source, index) => (
            <div className="schema-row" key={index}>
              <select value={source.credential_id} onChange={(event) => setSourceAt(sources, setSources, index, { ...source, credential_id: event.target.value })}>
                {props.credentials.map((credential) => (
                  <option key={credential.id} value={credential.id}>{accountLabel(credential, props.accountIds)}</option>
                ))}
              </select>
              <Input inputMode="decimal" value={source.coefficient} onChange={(event) => setSourceAt(sources, setSources, index, { ...source, coefficient: Number(event.target.value) })} />
              <label className="inline-check">
                <Checkbox checked={source.force_zero} onCheckedChange={(checked) => setSourceAt(sources, setSources, index, { ...source, force_zero: checked === true })} />
                Force zero
              </label>
              <Button variant="outline" type="button" onClick={() => setSources(sources.filter((_, sourceIndex) => sourceIndex !== index))}>Remove</Button>
            </div>
          ))}
        </div>
        <Button variant="outline" type="button" onClick={() => setSources([...sources, { credential_id: props.credentials[0]?.id ?? '', coefficient: 1, enabled: true, force_zero: false }])}>Add source</Button>
        <InlineError message={error} />
        <Button disabled={saving || !props.credentials.length || !sources.length} type="submit">
          {saving ? 'Saving...' : 'Save virtual account'}
        </Button>
      </form>
    </section>
  );
}

function FundsPage(props: {
  configs: FundConfig[];
  error: string | null;
  loading: boolean;
  snapshots: FundNavSnapshot[];
  virtualAccounts: VirtualAccountConfig[];
}) {
  const queryClient = useQueryClient();

  async function sampleFund(fundId: string) {
    const response = await fetch('/api/funds/sample?fund_id=' + encodeURIComponent(fundId), { method: 'POST' });
    if (!response.ok) {
      const body = await response.json().catch(() => null) as { message?: string } | null;
      throw new Error(body?.message ?? String(response.status) + ' ' + response.statusText);
    }
    await queryClient.invalidateQueries({ queryKey: ['json'] });
  }

  return (
    <div className="page-stack">
      <FundCreatePanel virtualAccounts={props.virtualAccounts} />

      <section className="metrics-grid compact" aria-label="Fund summary">
        <Metric label="Funds" value={props.configs.length.toString()} />
        <Metric label="Enabled" value={props.configs.filter((fund) => fund.enabled).length.toString()} />
        <Metric label="Snapshots loaded" value={props.snapshots.length.toString()} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel">
        <PanelTitle label="Fund configs" title="Virtual account NAV polling" action={props.configs.length + ' funds'} />
        <InlineError message={props.error} />
        <DataTable
          empty="No funds yet. Create a virtual account first, then bind a fund to it."
          headers={['Fund', 'Virtual account', 'Target', 'Interval', 'Last sample', 'Status', 'Action']}
          rows={props.configs.map((fund) => [
            fund.name,
            <AccountIdLink accountId={fund.account_id} key="account" />,
            fund.target_currency,
            fund.poll_interval_seconds + 's',
            fund.last_sampled_at ?? '-',
            fund.enabled ? 'Enabled' : 'Disabled',
            <Button variant="outline" key="sample" type="button" onClick={() => void sampleFund(fund.id)}>
              Sample now
            </Button>,
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle label="Recent NAV" title={props.configs[0]?.name ?? 'No fund selected'} action={props.snapshots.length + ' rows'} />
        <DataTable
          empty="No NAV snapshots recorded yet. The poller records enabled funds automatically."
          headers={['Time', 'Equity', 'Currency', 'Positions', 'Unpriced']}
          rows={props.snapshots.map((snapshot) => [
            snapshot.created_at,
            formatNumber(snapshot.equity),
            snapshot.target_currency,
            snapshot.positions_count.toString(),
            snapshot.unpriced_positions.toString(),
          ])}
        />
      </section>
    </div>
  );
}

function FundCreatePanel(props: { virtualAccounts: VirtualAccountConfig[] }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState('Local fund');
  const [accountId, setAccountId] = useState(props.virtualAccounts[0]?.account_id ?? '');
  const [targetCurrency, setTargetCurrency] = useState('USD');
  const [intervalSeconds, setIntervalSeconds] = useState(600);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!accountId && props.virtualAccounts[0]) {
      setAccountId(props.virtualAccounts[0].account_id);
    }
  }, [accountId, props.virtualAccounts]);

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);
    setSaving(true);

    try {
      const response = await fetch('/api/funds', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          name,
          account_id: accountId,
          enabled: true,
          target_currency: targetCurrency,
          poll_interval_seconds: intervalSeconds,
        }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? String(response.status) + ' ' + response.statusText);
      }
      await queryClient.invalidateQueries({ queryKey: ['json', '/api/funds'] });
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
          <p className="section-label">Create fund</p>
          <h2>Bind a virtual account</h2>
        </div>
        <span className="count-chip">Auto NAV</span>
      </div>
      <form className="credential-form" onSubmit={handleSubmit}>
        <label>
          Fund name
          <Input required value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <label>
          Virtual account
          <select required value={accountId} onChange={(event) => setAccountId(event.target.value)}>
            <option value="">Select a virtual account</option>
            {props.virtualAccounts.map((account) => (
              <option key={account.account_id} value={account.account_id}>{account.account_id} · {account.name}</option>
            ))}
          </select>
        </label>
        <label>
          Target currency
          <Input required value={targetCurrency} onChange={(event) => setTargetCurrency(event.target.value.toUpperCase())} />
        </label>
        <label>
          Poll interval seconds
          <Input min={60} inputMode="numeric" type="number" value={intervalSeconds} onChange={(event) => setIntervalSeconds(Number(event.target.value))} />
        </label>
        <InlineError message={error} />
        <Button disabled={saving || !props.virtualAccounts.length} type="submit">
          {saving ? 'Saving...' : 'Save fund'}
        </Button>
      </form>
    </section>
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
          <Input required placeholder="readonly-main" value={name} onChange={(event) => setName(event.target.value)} />
        </label>
        <div className="credential-field-grid">
          {fields.map((field) => (
            <label key={field}>
              {field}
              <Input
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
        <Button disabled={saving || !exchangeId || !name} type="submit">
          {saving ? 'Saving...' : 'Save credential'}
        </Button>
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
            <dd>Open Accounts or Positions to validate that the key can read live data.</dd>
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
          <LoadingStatus active={props.loading} label={props.loading ? 'Loading' : `${props.positions.length} rows`} />
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
          <LoadingStatus active={props.loading} label={props.loading ? 'Loading' : `${props.products.length} rows`} />
        </div>
        <InlineError message={props.error} />
        <DataTable
          empty="No products returned for this exchange."
          headers={['Product', 'Market', 'Name', 'Base', 'Quote', 'Price step', 'Volume step', 'Sides', 'Funding']}
          rows={props.products.slice(0, 250).map((item) => [
            <code key="product">{item.product_id}</code>,
            item.market_id ?? item.datasource_id,
            item.name ?? '-',
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            formatOptionalNumber(item.price_step),
            formatOptionalNumber(item.volume_step),
            sideLabel(item),
            item.no_interest_rate == null ? '-' : item.no_interest_rate ? 'No' : 'Yes',
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

function SectionLabel(props: { children: React.ReactNode }) {
  return <p className="mb-1 text-xs font-semibold uppercase tracking-normal text-muted-foreground">{props.children}</p>;
}

function Metric(props: { label: string; value: string; tone?: 'neutral' | 'good' | 'warn' }) {
  const valueClassName = props.tone === 'warn' ? 'text-destructive' : props.tone === 'good' ? 'text-primary' : 'text-foreground';

  return (
    <Card className="min-w-0" size="sm">
      <CardHeader>
        <CardDescription>{props.label}</CardDescription>
      </CardHeader>
      <CardContent className="min-w-0">
        <strong className={cn('block text-xl font-semibold leading-snug [overflow-wrap:anywhere]', valueClassName)}>{props.value}</strong>
      </CardContent>
    </Card>
  );
}

function DataTable(props: { empty: string; headers: string[]; rows: React.ReactNode[][] }) {
  if (props.rows.length === 0) {
    return (
      <Alert className="m-4">
        <AlertDescription>{props.empty}</AlertDescription>
      </Alert>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
            {props.headers.map((header) => (
            <TableHead key={header}>{header}</TableHead>
            ))}
        </TableRow>
      </TableHeader>
      <TableBody>
          {props.rows.map((row, rowIndex) => (
          <TableRow key={rowIndex}>
              {row.map((cell, cellIndex) => (
              <TableCell key={cellIndex}>{cell}</TableCell>
              ))}
          </TableRow>
          ))}
      </TableBody>
    </Table>
  );
}

function Badge(props: { children: React.ReactNode }) {
  return <UiBadge variant="secondary">{props.children}</UiBadge>;
}

function InlineError(props: { message: string | null }) {
  return props.message ? (
    <Alert className="m-4" variant="destructive">
      <AlertTitle>Request failed</AlertTitle>
      <AlertDescription>{props.message}</AlertDescription>
    </Alert>
  ) : null;
}

function Value(props: { value: number }) {
  return <span className={cn('font-medium', props.value < 0 && 'text-destructive')}>{formatNumber(props.value)}</span>;
}

function currentPage(pathname: string) {
  return pages.find((item) => item.path === pathname) ?? pages.find((item) => pathname.startsWith(`${item.path}/`)) ?? pages[0];
}

function sourceTypeLabel(sourceType: AccountSourceType) {
  if (sourceType === 'credential') {
    return 'Real EX credential';
  }
  if (sourceType === 'virtual') {
    return 'Virtual account';
  }
  return 'Custom account source';
}

function scrollToAccountSource(sourceType: AccountSourceType) {
  document.getElementById('account-source-' + sourceType)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
}

function accountIdForCredential(credential: Credential, accountIds: AccountIds) {
  return accountIds[credential.id] ?? fallbackAccountId(credential);
}

function accountIdsFromRefs(refs: AccountRef[]) {
  return Object.fromEntries(
    refs.flatMap((accountRef) => (accountRef.account_id ? [[accountRef.credential_id, accountRef.account_id]] : [])),
  );
}

function credentialAccounts(credentials: Credential[], refs: AccountRef[]) {
  const refsByCredential = new Map(refs.map((accountRef) => [accountRef.credential_id, accountRef]));
  return credentials.map((credential) => {
    const accountRef = refsByCredential.get(credential.id);
    return {
      accountId: accountRef?.account_id ?? fallbackAccountId(credential),
      credential,
      error: accountRef?.error ?? null,
      positions: [],
      sourceLabel: credential.exchange,
      sourceType: 'credential' as const,
    };
  });
}

function virtualAccountSnapshots(configs: VirtualAccountConfig[]) {
  return configs
    .filter((config) => config.enabled)
    .map((config) => ({
      accountId: config.account_id,
      credential: virtualAccountCredential(config),
      error: null,
      positions: [],
      sourceLabel: 'Linear composer',
      sourceType: 'virtual' as const,
    }));
}

function virtualAccountCredential(config: VirtualAccountConfig): Credential {
  return {
    id: config.account_id,
    exchange: 'VIRTUAL',
    name: config.name,
    has_payload: false,
    created_at: config.created_at,
    updated_at: config.updated_at,
  };
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

function accountInfoToAccountSnapshot(account: AccountInfo): AccountSnapshot {
  return {
    accountId: account.account_id,
    credential: accountCredential(account),
    error: null,
    positions: account.positions,
    sourceLabel: 'Live read',
    sourceType: 'credential',
  };
}

function accountCredential(account: AccountInfo): Credential {
  const exchange = account.account_id.split('/')[0] || 'ACCOUNT';
  return {
    id: account.account_id,
    exchange,
    name: account.account_id,
    has_payload: false,
    created_at: '',
    updated_at: '',
  };
}

function credentialPayload(fields: string[], values: Record<string, string>) {
  return Object.fromEntries(fields.map((field) => [field, values[field] ?? '']));
}

function setSourceAt(
  sources: VirtualAccountSource[],
  setSources: (sources: VirtualAccountSource[]) => void,
  index: number,
  source: VirtualAccountSource,
) {
  setSources(sources.map((item, itemIndex) => (itemIndex === index ? source : item)));
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

function summarizeAccountPositions(positions: Position[]) {
  return positions.reduce(
    (summary, item) => {
      addCurrencyTotal(summary.notionalByCurrency, item.notional_currency, notionalValue(item));
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

function summarizeAccountAssets(positions: Position[]) {
  const rows = new Map<
    string,
    { currency: string; freeVolume: number; notionalValue: number; pnl: number; productId: string; rows: number; volume: number }
  >();
  for (const position of positions) {
    const currency = position.notional_currency ?? 'UNKNOWN';
    const rowKey = `${position.product_id}\u0000${currency}`;
    const current = rows.get(rowKey) ?? {
      currency,
      freeVolume: 0,
      notionalValue: 0,
      pnl: 0,
      productId: position.product_id,
      rows: 0,
      volume: 0,
    };
    current.rows += 1;
    current.volume += finiteNumber(position.volume);
    current.freeVolume += finiteNumber(position.free_volume);
    current.notionalValue += finiteNumber(notionalValue(position));
    current.pnl += finiteNumber(position.floating_profit);
    rows.set(rowKey, current);
  }

  return Array.from(rows.values())
    .sort((a, b) => Math.abs(b.notionalValue) - Math.abs(a.notionalValue));
}

function notionalValue(position: Position) {
  if (position.notional != null) {
    return finiteNumber(Number(position.notional));
  }

  return position.valuation ?? position.notional_value;
}

function formatPositionNotional(position: Position) {
  return `${formatNumber(notionalValue(position))} ${position.notional_currency ?? position.quote_currency ?? ''}`.trim();
}

function addCurrencyTotal(totals: Map<string, number>, currency: string | null, value: number) {
  const key = currency ?? 'UNKNOWN';
  totals.set(key, (totals.get(key) ?? 0) + finiteNumber(value));
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

function convertCurrencyTotals(totals: Map<string, number>, target: string, edges: CurrencyRateEdge[]) {
  return Array.from(totals.entries()).reduce(
    (result, [currency, value]) => {
      const rate = currencyRate(edges, currency, target);
      if (rate == null) {
        result.unconverted.push(currency);
      } else {
        result.value += finiteNumber(value) * rate;
      }
      return result;
    },
    { value: 0, unconverted: [] as string[] },
  );
}

function currencyRate(edges: CurrencyRateEdge[], from: string, to: string) {
  if (from === to) {
    return 1;
  }
  const queue: Array<{ currency: string; rate: number }> = [{ currency: from, rate: 1 }];
  const seen = new Set([from]);

  for (let index = 0; index < queue.length; index += 1) {
    const current = queue[index];
    for (const edge of edges) {
      if (edge.base_currency !== current.currency || seen.has(edge.quote_currency) || edge.rate <= 0) {
        continue;
      }
      const nextRate = current.rate * edge.rate;
      if (edge.quote_currency === to) {
        return nextRate;
      }
      seen.add(edge.quote_currency);
      queue.push({ currency: edge.quote_currency, rate: nextRate });
    }
  }
  return null;
}

function formatConvertedValue(result: { value: number; unconverted: string[] }, currency: string) {
  const suffix = result.unconverted.length > 0 ? ` (${result.unconverted.length} unpriced)` : '';
  return `${formatNumber(result.value)} ${currency}${suffix}`;
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
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <App />
      </HashRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
