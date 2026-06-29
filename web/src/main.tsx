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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog';
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
  Ban,
  ChevronDown,
  CircleCheck,
  Download,
  Eye,
  FileText,
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
  settlement_currency?: string | null;
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

type FundStatementSummary = {
  totals: FundStatementTotals;
  investors: FundStatementInvestor[];
  recent_orders: FundStatementOrder[];
  latest_equity: FundStatementEquity | null;
  reconciliation: FundEquityReconciliation | null;
  tax_modes: FundStatementTaxMode[];
  tax_threshold_adjustments: FundTaxThresholdAdjustment[];
};

type FundStatementTotals = {
  events: number;
  orders: number;
  order_deposit: number;
  inflow_count: number;
  inflow_amount: number;
  outflow_count: number;
  outflow_amount: number;
  equity_points: number;
  investors: number;
  tax_modes: number;
  tax_threshold_adjustments: number;
  tax_threshold_amount: number;
  overdrawn_cash_flows: number;
  overdrawn_investors: number;
  capped_cash_flows: number;
  capped_units: number;
  capped_cash_amount: number;
};

type FundStatementInvestor = {
  name: string;
  referrer: string | null;
  tax_rate: number | null;
  referrer_rebate_rate: number | null;
  tax_threshold: number | null;
  updated_at: string;
  source_event_index: number;
};

type FundStatementOrder = {
  event_index: number;
  investor_name: string;
  deposit: number;
  effective_deposit: number;
  capped_cash_amount: number;
  direction: string;
  nav_per_unit: number;
  requested_unit_delta: number;
  unit_delta: number;
  capped_units: number;
  investor_units_after: number;
  total_units_after: number;
  updated_at: string;
};

type FundStatementEquity = {
  event_index: number;
  equity: number;
  updated_at: string;
};

type FundStatementTaxMode = {
  event_index: number;
  mode: string;
  comment: string | null;
  updated_at: string;
};

type FundTaxThresholdAdjustment = {
  event_index: number;
  investor_name: string;
  amount: number;
  comment: string | null;
  updated_at: string;
};

type FundEquityReconciliation = {
  legacy_equity: number;
  legacy_updated_at: string;
  nav_equity: number;
  nav_created_at: string;
  delta: number;
  delta_rate: number | null;
};

type FundSettlementPreview = {
  fund_id: string;
  latest_equity: FundStatementEquity | null;
  basis: FundSettlementBasis | null;
  total_deposit: number;
  total_units: number;
  total_tax: number;
  total_referrer_rebate: number;
  totals: FundSettlementTotals;
  investor_taxes: FundInvestorTax[];
  referrer_rebates: FundReferrerRebate[];
  investors: FundInvestorSettlement[];
};

type FundInvestorSettlement = {
  name: string;
  referrer: string | null;
  deposit: number;
  units: number;
  ownership: number;
  gross_equity: number;
  profit: number;
  tax_threshold: number;
  tax_rate: number;
  tax: number;
  referrer_rebate_rate: number;
  referrer_rebate: number;
  capped_cash_amount: number;
  net_equity: number;
};

type FundSettlementRun = {
  id: string;
  fund_id: string;
  equity_event_index: number;
  equity: number;
  equity_updated_at: string;
  basis_source: string;
  basis_id: string;
  basis_updated_at: string;
  total_deposit: number;
  total_units: number;
  total_tax: number;
  total_referrer_rebate: number;
  capped_cash_flows: number;
  capped_units: number;
  capped_cash_amount: number;
  investor_count: number;
  status: string;
  status_updated_at: string | null;
  created_at: string;
};

type FundSettlementRunDetail = {
  run: FundSettlementRun;
  investors: FundInvestorSettlement[];
  totals: FundSettlementTotals;
  investor_taxes: FundInvestorTax[];
  referrer_rebates: FundReferrerRebate[];
};

type FundSettlementTotals = {
  gross_equity: number;
  net_equity: number;
  tax: number;
  referrer_rebate: number;
  retained_tax: number;
  overdrawn_investors: number;
  capped_cash_flows: number;
  capped_units: number;
  capped_cash_amount: number;
};

type FundSettlementBasis = {
  source: string;
  id: string;
  equity: number;
  updated_at: string;
};

type FundInvestorTax = {
  investor: string;
  tax: number;
};

type FundReferrerRebate = {
  referrer: string;
  rebate: number;
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
          <Route path="/funds/detail" element={<PageBoundary><FundDetailRoute /></PageBoundary>} />
          <Route path="/funds/settlement" element={<PageBoundary><SettlementReportRoute /></PageBoundary>} />
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

  return (
    <RefreshScope resources={[funds, virtualAccounts]}>
      <FundListPage
        configs={funds.data ?? emptyFundConfigs}
        error={funds.error ?? virtualAccounts.error}
        loading={funds.loading || virtualAccounts.loading}
        virtualAccounts={virtualAccounts.data ?? emptyVirtualAccountConfigs}
      />
    </RefreshScope>
  );
}

function FundDetailRoute() {
  const [params] = useSearchParams();
  const fundId = params.get('fund_id') ?? '';
  const funds = useJson<FundConfig[]>('/api/funds');
  const nav = useJson<FundNavSnapshot[]>(fundId ? `/api/fund-nav?fund_id=${encodeURIComponent(fundId)}&limit=100` : null);
  const statements = useJson<FundStatementSummary>(fundId ? `/api/fund-statements?fund_id=${encodeURIComponent(fundId)}` : null);
  const settlement = useJson<FundSettlementPreview>(fundId ? `/api/fund-settlement-preview?fund_id=${encodeURIComponent(fundId)}` : null);
  const settlementRuns = useJson<FundSettlementRun[]>(fundId ? `/api/fund-settlement-runs?fund_id=${encodeURIComponent(fundId)}` : null);

  return (
    <RefreshScope resources={[funds, nav, statements, settlement, settlementRuns]}>
      <FundDetailPage
        configs={funds.data ?? emptyFundConfigs}
        fundId={fundId}
        loading={funds.loading || nav.loading || statements.loading || settlement.loading || settlementRuns.loading}
        navError={nav.error}
        settlementError={settlement.error}
        settlementPreview={settlement.data ?? null}
        settlementRuns={settlementRuns.data ?? []}
        settlementRunsError={settlementRuns.error}
        statementError={statements.error}
        statementSummary={statements.data ?? null}
        snapshots={nav.data ?? []}
      />
    </RefreshScope>
  );
}

function SettlementReportRoute() {
  const [params] = useSearchParams();
  const runId = params.get('run_id') ?? '';
  const detail = useJson<FundSettlementRunDetail>(runId ? '/api/fund-settlement-runs/detail?run_id=' + encodeURIComponent(runId) : null);

  return (
    <RefreshScope resources={[detail]}>
      <SettlementReportPage detail={detail.data ?? null} error={detail.error} loading={detail.loading} runId={runId} />
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
          headers={['AccountID', 'Source type', 'Name', 'Protocol', 'Positions', 'Assets', 'Equity USD', 'Floating P/L', 'Status', 'Action']}
          rows={props.accounts.map((account) => {
            const summary = summarizeAccountPositions(account.positions);
            return [
              <AccountIdLink accountId={account.accountId} key="account" />,
              sourceTypeLabel(account.sourceType),
              account.credential.name,
              <Badge key="protocol">{account.sourceLabel}</Badge>,
              summary.total.toString(),
              summary.assets.toString(),
              formatConvertedValue(convertCurrencyTotals(summary.equityByCurrency, 'USD', props.rateEdges), 'USD'),
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
        <div className="account-source-actions">
          <AccountSourceCreateDialog
            description="Save a read-only exchange credential locally. Payload values stay server-side after creation."
            title="Add real EX credential"
            trigger="Add credential"
          >
            <CredentialCreatePanel exchanges={props.exchanges} onCreated={props.onCredentialCreated} />
          </AccountSourceCreateDialog>
        </div>
        <CredentialSecurityPanel />
        <CredentialInventory accountIds={props.accountIds} credentials={props.credentials} />
        <CredentialSchemaPanel exchanges={props.exchanges} />
      </div>

      <div className="account-source-section" id="account-source-virtual">
        <PanelTitle label="Account source" title="Virtual accounts" action={props.virtualAccounts.length + ' configs'} />
        <div className="account-source-actions">
          <AccountSourceCreateDialog
            description="Create a linear composition account from existing exchange credentials."
            title="Create virtual account"
            trigger="Create virtual account"
          >
            <VirtualAccountCreatePanel accountIds={props.accountIds} credentials={props.credentials} />
          </AccountSourceCreateDialog>
        </div>
        <VirtualAccountInventory configs={props.virtualAccounts} />
      </div>

      <div className="account-source-section" id="account-source-custom">
        <PanelTitle label="Account source" title="Custom account sources" action={props.customSources.length + ' sources'} />
        <div className="account-source-actions">
          <AccountSourceCreateDialog
            description="Register another 1Exchange-compatible server as a remote account source."
            title="Register custom account source"
            trigger="Register source"
          >
            <CustomAccountSourceForm />
          </AccountSourceCreateDialog>
        </div>
        <CustomAccountSourceInventory sources={props.customSources} />
      </div>
    </div>
  );
}

function AccountSourceCreateDialog(props: { children: React.ReactNode; description: string; title: string; trigger: string }) {
  return (
    <Dialog>
      <DialogTrigger render={<Button type="button" />}>{props.trigger}</DialogTrigger>
      <DialogContent className="max-h-[min(720px,calc(100vh-2rem))] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>{props.title}</DialogTitle>
          <DialogDescription>{props.description}</DialogDescription>
        </DialogHeader>
        {props.children}
      </DialogContent>
    </Dialog>
  );
}

function CustomAccountSourceForm() {
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
    <form className="credential-form dialog-form" onSubmit={handleSubmit}>
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
  );
}

function CustomAccountSourceInventory(props: { sources: CustomAccountSource[] }) {
  return (
    <section className="panel">
      <PanelTitle label="Custom sources" title="Remote 1Exchange protocols" action={props.sources.length + ' sources'} />
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
  const usdValue = convertCurrencyTotals(summary.equityByCurrency, 'USD', props.rateEdges);

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
        <Metric label="Equity value" value={formatCurrencyBreakdown(summary.equityByCurrency)} />
        <Metric label="USD equity" value={formatConvertedValue(usdValue, 'USD')} tone={usdValue.unconverted.length > 0 ? 'warn' : 'neutral'} />
        <Metric label="Exposure" value={formatCurrencyBreakdown(summary.exposureByCurrency)} />
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
          headers={['Product', 'Exposure currency', 'Settlement', 'Rows', 'Volume', 'Free', 'Equity', 'Exposure', 'Floating P/L']}
          rows={assetRows.map((row) => [
            <code key="product">{row.productId}</code>,
            row.exposureCurrency,
            row.settlementCurrency,
            row.rows.toString(),
            formatNumber(row.volume),
            formatNumber(row.freeVolume),
            formatNumber(row.equityValue),
            formatNumber(row.exposureValue),
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
          headers={['Position', 'Product', 'Base', 'Quote', 'Settle', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'Equity', 'Exposure', 'P/L']}
          rows={account.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            settlementCurrency(item),
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatPositionEquity(item),
            formatPositionExposure(item),
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
      <form className="credential-form dialog-form" onSubmit={handleSubmit}>
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
  );
}

function FundListPage(props: {
  configs: FundConfig[];
  error: string | null;
  loading: boolean;
  virtualAccounts: VirtualAccountConfig[];
}) {
  return (
    <div className="page-stack">
      <FundCreatePanel virtualAccounts={props.virtualAccounts} />

      <section className="metrics-grid compact" aria-label="Fund summary">
        <Metric label="Funds" value={props.configs.length.toString()} />
        <Metric label="Enabled" value={props.configs.filter((fund) => fund.enabled).length.toString()} />
        <Metric label="Virtual accounts" value={props.virtualAccounts.length.toString()} />
        <Metric label="Status" value={props.loading ? 'Loading' : 'Ready'} />
      </section>

      <section className="panel">
        <PanelTitle label="Fund configs" title="Virtual account NAV polling" action={props.configs.length + ' funds'} />
        <InlineError message={props.error} />
        <DataTable
          empty="No funds yet. Create a virtual account first, then bind a fund to it."
          headers={['Fund', 'Virtual account', 'Target', 'Interval', 'Last sample', 'Status', 'Action']}
          rows={props.configs.map((fund) => [
            <FundLink fund={fund} key="fund" />,
            <AccountIdLink accountId={fund.account_id} key="account" />,
            fund.target_currency,
            fund.poll_interval_seconds + 's',
            fund.last_sampled_at ?? '-',
            fund.enabled ? 'Enabled' : 'Disabled',
            <Link className="secondary-link" key="action" to={fundDetailPath(fund.id)}>Open</Link>,
          ])}
        />
      </section>
    </div>
  );
}

function FundDetailPage(props: {
  configs: FundConfig[];
  fundId: string;
  loading: boolean;
  navError: string | null;
  settlementError: string | null;
  settlementPreview: FundSettlementPreview | null;
  settlementRuns: FundSettlementRun[];
  settlementRunsError: string | null;
  statementError: string | null;
  statementSummary: FundStatementSummary | null;
  snapshots: FundNavSnapshot[];
}) {
  const queryClient = useQueryClient();
  const [selectedSettlementRunId, setSelectedSettlementRunId] = useState<string | null>(null);
  const [settlementRunError, setSettlementRunError] = useState<string | null>(null);
  const [settlementRunActionId, setSettlementRunActionId] = useState<string | null>(null);
  const [settlementRunSaving, setSettlementRunSaving] = useState(false);
  const fund = props.configs.find((item) => item.id === props.fundId);
  const latestSnapshot = props.snapshots[0];
  const statement = props.statementSummary;
  const settlement = props.settlementPreview;

  async function sampleFund() {
    const response = await fetch('/api/funds/sample?fund_id=' + encodeURIComponent(props.fundId), { method: 'POST' });
    if (!response.ok) {
      const body = await response.json().catch(() => null) as { message?: string } | null;
      throw new Error(body?.message ?? String(response.status) + ' ' + response.statusText);
    }
    await queryClient.invalidateQueries({ queryKey: ['json'] });
  }

  async function createSettlementRun() {
    setSettlementRunError(null);
    setSettlementRunSaving(true);
    try {
      const response = await fetch('/api/fund-settlement-runs', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ fund_id: props.fundId }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? String(response.status) + ' ' + response.statusText);
      }
      await response.json() as FundSettlementRunDetail;
      await queryClient.invalidateQueries({ queryKey: ['json'] });
    } catch (error) {
      setSettlementRunError(error instanceof Error ? error.message : String(error));
    } finally {
      setSettlementRunSaving(false);
    }
  }

  async function updateSettlementRunStatus(runId: string, action: 'confirm' | 'void') {
    setSettlementRunError(null);
    setSettlementRunActionId(runId);
    try {
      const response = await fetch('/api/fund-settlement-runs/' + action, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ run_id: runId }),
      });
      if (!response.ok) {
        const body = await response.json().catch(() => null) as { message?: string } | null;
        throw new Error(body?.message ?? String(response.status) + ' ' + response.statusText);
      }
      await response.json() as FundSettlementRunDetail;
      await queryClient.invalidateQueries({ queryKey: ['json'] });
    } catch (error) {
      setSettlementRunError(error instanceof Error ? error.message : String(error));
    } finally {
      setSettlementRunActionId(null);
    }
  }

  if (!props.fundId) {
    return <FundDetailEmpty title="Select a fund" message="Open the Funds page and choose a fund to inspect NAV records." />;
  }

  if (!fund) {
    return props.loading
      ? <FundDetailEmpty title="Loading fund" message="Fund config is loading from the local registry." />
      : <FundDetailEmpty title="Fund not found" message="This fund is not available in the current local registry." />;
  }

  return (
    <div className="page-stack">
      <section className="panel account-detail-head">
        <div>
          <p className="section-label">Fund detail</p>
          <h2>{fund.name}</h2>
        </div>
        <div className="flex flex-wrap items-center justify-end gap-2">
          <Button variant="default" type="button" disabled={settlementRunSaving} onClick={() => void createSettlementRun()}>
            {settlementRunSaving ? 'Creating...' : 'Create settlement run'}
          </Button>
          <Button variant="outline" type="button" onClick={() => void sampleFund()}>Sample now</Button>
          <Link className="secondary-link" to="/funds">Back to funds</Link>
        </div>
      </section>

      <section className="metrics-grid compact" aria-label="Fund detail summary">
        <Metric label="Status" value={fund.enabled ? 'Enabled' : 'Disabled'} />
        <Metric label="Target" value={fund.target_currency} />
        <Metric label="Snapshots" value={props.snapshots.length.toString()} />
        <Metric label="Latest equity" value={latestSnapshot ? formatNumber(latestSnapshot.equity) : '-'} />
        <Metric label="Last sample" value={fund.last_sampled_at ?? '-'} />
      </section>

      <section className="metrics-grid compact" aria-label="Fund statement summary">
        <Metric label="Statement events" value={statement ? statement.totals.events.toString() : '-'} />
        <Metric label="Investors" value={statement ? statement.totals.investors.toString() : '-'} />
        <Metric label="Cash flows" value={statement ? statement.totals.orders.toString() : '-'} />
        <Metric label="Investor inflows" value={statement ? formatNumber(statement.totals.inflow_amount) : '-'} />
        <Metric label="Investor outflows" value={statement ? formatNumber(statement.totals.outflow_amount) : '-'} />
        <Metric label="Net cash flow" value={statement ? formatNumber(statement.totals.order_deposit) : '-'} />
        <Metric label="Tax threshold" value={statement ? formatNumber(statement.totals.tax_threshold_amount) : '-'} />
        <Metric
          label="Overdrawn flows"
          value={statement ? statement.totals.overdrawn_cash_flows.toString() : '-'}
          tone={statement?.totals.overdrawn_cash_flows ? 'warn' : 'neutral'}
        />
        <Metric
          label="Overdrawn investors"
          value={statement ? statement.totals.overdrawn_investors.toString() : '-'}
          tone={statement?.totals.overdrawn_investors ? 'warn' : 'neutral'}
        />
        <Metric
          label="Capped flows"
          value={statement ? statement.totals.capped_cash_flows.toString() : '-'}
          tone={statement?.totals.capped_cash_flows ? 'warn' : 'neutral'}
        />
        <Metric label="Capped units" value={statement ? formatNumber(statement.totals.capped_units) : '-'} />
        <Metric label="Capped cash" value={statement ? formatNumber(statement.totals.capped_cash_amount) : '-'} />
        <Metric label="Legacy equity" value={statement?.latest_equity ? formatNumber(statement.latest_equity.equity) : '-'} />
        <Metric label="Tax modes" value={statement ? statement.totals.tax_modes.toString() : '-'} />
      </section>

      <section className="metrics-grid compact" aria-label="Fund equity reconciliation">
        <Metric label="NAV equity" value={statement?.reconciliation ? formatNumber(statement.reconciliation.nav_equity) : '-'} />
        <Metric
          label="NAV delta"
          value={statement?.reconciliation ? formatNumber(statement.reconciliation.delta) : '-'}
          tone={statement?.reconciliation && Math.abs(statement.reconciliation.delta) > 0.01 ? 'warn' : 'neutral'}
        />
        <Metric
          label="NAV delta rate"
          value={statement?.reconciliation?.delta_rate == null ? '-' : formatPercent(statement.reconciliation.delta_rate)}
          tone={statement?.reconciliation?.delta_rate && Math.abs(statement.reconciliation.delta_rate) > 0.0001 ? 'warn' : 'neutral'}
        />
        <Metric label="Legacy time" value={statement?.reconciliation ? formatDate(statement.reconciliation.legacy_updated_at) : '-'} />
        <Metric label="NAV time" value={statement?.reconciliation ? formatDate(statement.reconciliation.nav_created_at) : '-'} />
      </section>

      <section className="metrics-grid compact" aria-label="Fund settlement preview">
        <Metric label="Basis" value={settlement?.basis ? settlementBasisLabel(settlement.basis.source) : '-'} />
        <Metric label="Basis time" value={settlement?.basis ? formatDate(settlement.basis.updated_at) : '-'} />
        <Metric label="Issued units" value={settlement ? formatNumber(settlement.total_units) : '-'} />
        <Metric label="Total deposit" value={settlement ? formatNumber(settlement.total_deposit) : '-'} />
        <Metric label="Gross equity" value={settlement ? formatNumber(settlement.totals.gross_equity) : '-'} />
        <Metric label="Net equity" value={settlement ? formatNumber(settlement.totals.net_equity) : '-'} />
        <Metric label="Estimated tax" value={settlement ? formatNumber(settlement.total_tax) : '-'} />
        <Metric label="Referrer rebate" value={settlement ? formatNumber(settlement.total_referrer_rebate) : '-'} />
        <Metric label="Retained tax" value={settlement ? formatNumber(settlement.totals.retained_tax) : '-'} />
        <Metric
          label="Capped flows"
          value={settlement ? settlement.totals.capped_cash_flows.toString() : '-'}
          tone={settlement?.totals.capped_cash_flows ? 'warn' : 'neutral'}
        />
        <Metric label="Capped cash" value={settlement ? formatNumber(settlement.totals.capped_cash_amount) : '-'} />
        <Metric
          label="Overdrawn investors"
          value={settlement ? settlement.totals.overdrawn_investors.toString() : '-'}
          tone={settlement?.totals.overdrawn_investors ? 'warn' : 'neutral'}
        />
      </section>

      <section className="panel">
        <div className="account-detail-grid">
          <DetailItem label="Fund ID" value={fund.id} monospace />
          <DetailItem label="Virtual account" value={fund.account_id} monospace />
          <DetailItem label="Target currency" value={fund.target_currency} />
          <DetailItem label="Poll interval" value={fund.poll_interval_seconds + 's'} />
          <DetailItem label="Created" value={formatDate(fund.created_at)} />
          <DetailItem label="Updated" value={formatDate(fund.updated_at)} />
        </div>
      </section>

      <section className="panel">
        <PanelTitle
          label="Settlement preview"
          title="Investor allocation"
          action={settlement ? settlement.investors.length + ' investors' : undefined}
        />
        <InlineError message={props.settlementError} />
        <DataTable
          empty="No settlement preview is available for this fund."
          headers={['Investor', 'Referrer', 'Deposit', 'Capped cash', 'Ownership', 'Gross equity', 'Profit', 'Tax', 'Rebate', 'Net equity']}
          rows={(settlement?.investors ?? []).map((investor) => [
            investor.name,
            investor.referrer ?? '-',
            formatNumber(investor.deposit),
            formatNumber(investor.capped_cash_amount),
            formatPercent(investor.ownership),
            formatNumber(investor.gross_equity),
            <Value key="profit" value={investor.profit} />,
            formatNumber(investor.tax),
            formatNumber(investor.referrer_rebate),
            formatNumber(investor.net_equity),
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Settlement preview"
          title="Referrer rebates"
          action={settlement ? settlement.referrer_rebates.length + ' referrers' : undefined}
        />
        <DataTable
          empty="No referrer rebate is estimated for this settlement."
          headers={['Referrer', 'Estimated rebate']}
          rows={(settlement?.referrer_rebates ?? []).map((rebate) => [
            rebate.referrer,
            formatNumber(rebate.rebate),
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Settlement preview"
          title="Tax payable"
          action={settlement ? settlement.investor_taxes.length + ' investors' : undefined}
        />
        <DataTable
          empty="No investor tax is estimated for this settlement."
          headers={['Investor', 'Estimated tax']}
          rows={(settlement?.investor_taxes ?? []).map((tax) => [
            tax.investor,
            formatNumber(tax.tax),
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Settlement runs"
          title="Draft history"
          action={props.settlementRuns.length + ' runs'}
        />
        <InlineError message={settlementRunError ?? props.settlementRunsError} />
        <DataTable
          empty="No settlement runs have been created yet."
          headers={['Created', 'Status', 'Status time', 'Basis', 'Equity', 'Investors', 'Deposit', 'Capped cash', 'Tax', 'Rebate', 'Run ID', 'Action']}
          rows={props.settlementRuns.map((run) => [
            formatDate(run.created_at),
            <UiBadge key="status" variant={run.status === 'confirmed' ? 'default' : 'secondary'}>{run.status}</UiBadge>,
            run.status_updated_at ? formatDate(run.status_updated_at) : '-',
            settlementBasisLabel(run.basis_source),
            formatNumber(run.equity),
            run.investor_count.toString(),
            formatNumber(run.total_deposit),
            formatNumber(run.capped_cash_amount),
            formatNumber(run.total_tax),
            formatNumber(run.total_referrer_rebate),
            <span className="font-mono text-xs" key="run-id">{run.id}</span>,
            <SettlementRunActions
              actioning={settlementRunActionId === run.id}
              key="action"
              onConfirm={() => void updateSettlementRunStatus(run.id, 'confirm')}
              onInspect={() => setSelectedSettlementRunId(run.id)}
              onVoid={() => void updateSettlementRunStatus(run.id, 'void')}
              runId={run.id}
              status={run.status}
            />,
          ])}
        />
      </section>

      <SettlementRunDetailDialog
        onOpenChange={(open) => {
          if (!open) {
            setSelectedSettlementRunId(null);
          }
        }}
        open={selectedSettlementRunId !== null}
        runId={selectedSettlementRunId}
      />

      <section className="panel">
        <PanelTitle
          label="Legacy statement"
          title="Investor ledger"
          action={statement ? statement.investors.length + ' investors' : undefined}
        />
        <InlineError message={props.statementError} />
        <DataTable
          empty="No imported statement investors are available for this fund."
          headers={['Investor', 'Referrer', 'Tax rate', 'Rebate rate', 'Tax threshold', 'Updated']}
          rows={(statement?.investors ?? []).map((investor) => [
            investor.name,
            investor.referrer ?? '-',
            formatPercent(investor.tax_rate),
            formatPercent(investor.referrer_rebate_rate),
            formatOptionalNumber(investor.tax_threshold),
            formatDate(investor.updated_at),
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Legacy statement"
          title="Tax threshold adjustments"
          action={statement ? statement.tax_threshold_adjustments.length + ' adjustments' : undefined}
        />
        <DataTable
          empty="No tax threshold adjustments are available for this fund."
          headers={['Time', 'Investor', 'Amount', 'Comment', 'Event']}
          rows={(statement?.tax_threshold_adjustments ?? []).map((adjustment) => [
            formatDate(adjustment.updated_at),
            adjustment.investor_name,
            formatNumber(adjustment.amount),
            adjustment.comment ?? '-',
            '#' + adjustment.event_index,
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Legacy statement"
          title="Recent cash flows"
          action={statement ? statement.recent_orders.length + ' flows' : undefined}
        />
        <DataTable
          empty="No imported statement orders are available for this fund."
          headers={['Time', 'Investor', 'Direction', 'Amount', 'Effective amount', 'Capped cash', 'NAV/unit', 'Requested units', 'Unit delta', 'Capped units', 'Investor units', 'Fund units', 'Event']}
          rows={(statement?.recent_orders ?? []).map((order) => [
            formatDate(order.updated_at),
            order.investor_name,
            cashFlowDirectionLabel(order.direction),
            <Value key="amount" value={order.deposit} />,
            <Value key="effective-amount" value={order.effective_deposit} />,
            formatNumber(order.capped_cash_amount),
            formatNumber(order.nav_per_unit),
            <Value key="requested-unit-delta" value={order.requested_unit_delta} />,
            <Value key="unit-delta" value={order.unit_delta} />,
            formatNumber(order.capped_units),
            formatNumber(order.investor_units_after),
            formatNumber(order.total_units_after),
            '#' + order.event_index,
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle
          label="Legacy statement"
          title="Tax modes"
          action={statement ? statement.tax_modes.length + ' markers' : undefined}
        />
        <DataTable
          empty="No imported tax mode markers are available for this fund."
          headers={['Time', 'Mode', 'Comment', 'Event']}
          rows={(statement?.tax_modes ?? []).map((mode) => [
            formatDate(mode.updated_at),
            mode.mode,
            mode.comment ?? '-',
            '#' + mode.event_index,
          ])}
        />
      </section>

      <section className="panel">
        <PanelTitle label="Recent NAV" title={fund.name} action={props.snapshots.length + ' rows'} />
        <InlineError message={props.navError} />
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

function SettlementRunActions(props: {
  actioning: boolean;
  onConfirm: () => void;
  onInspect: () => void;
  onVoid: () => void;
  runId: string;
  status: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <Button size="sm" variant="outline" type="button" onClick={props.onInspect}>
        <Eye data-icon="inline-start" />
        View
      </Button>
      <Button size="sm" variant="outline" type="button" render={<Link to={settlementReportPath(props.runId)} />}>
        <FileText data-icon="inline-start" />
        Report
      </Button>
      {props.status === 'draft' ? (
        <>
          <Button size="sm" type="button" disabled={props.actioning} onClick={props.onConfirm}>
            <CircleCheck data-icon="inline-start" />
            Confirm
          </Button>
          <Button size="sm" variant="outline" type="button" disabled={props.actioning} onClick={props.onVoid}>
            <Ban data-icon="inline-start" />
            Void
          </Button>
        </>
      ) : null}
      <Button size="sm" variant="outline" type="button" render={<a href={settlementRunExportPath(props.runId)} />}>
        <Download data-icon="inline-start" />
        CSV
      </Button>
    </div>
  );
}

function SettlementRunDetailDialog(props: {
  onOpenChange: (open: boolean) => void;
  open: boolean;
  runId: string | null;
}) {
  const detail = useJson<FundSettlementRunDetail>(props.runId ? '/api/fund-settlement-runs/detail?run_id=' + encodeURIComponent(props.runId) : null);
  const run = detail.data?.run;

  return (
    <Dialog open={props.open} onOpenChange={props.onOpenChange}>
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>Settlement run detail</DialogTitle>
          <DialogDescription>{run ? run.status + ' · ' + formatDate(run.status_updated_at ?? run.created_at) : 'Loading settlement run'}</DialogDescription>
        </DialogHeader>
        <InlineError message={detail.error} />
        {run ? (
          <div className="flex flex-col gap-4">
            <section className="metrics-grid compact" aria-label="Settlement run detail summary">
              <Metric label="Status" value={run.status} />
              <Metric label="Status time" value={formatDate(run.status_updated_at ?? run.created_at)} />
              <Metric label="Basis" value={settlementBasisLabel(run.basis_source)} />
              <Metric label="Basis time" value={formatDate(run.basis_updated_at)} />
              <Metric label="Equity" value={formatNumber(run.equity)} />
              <Metric label="Investors" value={run.investor_count.toString()} />
              <Metric label="Gross equity" value={detail.data ? formatNumber(detail.data.totals.gross_equity) : '-'} />
              <Metric label="Net equity" value={detail.data ? formatNumber(detail.data.totals.net_equity) : '-'} />
              <Metric label="Tax" value={formatNumber(run.total_tax)} />
              <Metric label="Rebate" value={formatNumber(run.total_referrer_rebate)} />
              <Metric label="Retained tax" value={detail.data ? formatNumber(detail.data.totals.retained_tax) : '-'} />
              <Metric
                label="Capped flows"
                value={run.capped_cash_flows.toString()}
                tone={run.capped_cash_flows ? 'warn' : 'neutral'}
              />
              <Metric label="Capped cash" value={formatNumber(run.capped_cash_amount)} />
              <Metric
                label="Overdrawn investors"
                value={detail.data ? detail.data.totals.overdrawn_investors.toString() : '-'}
                tone={detail.data?.totals.overdrawn_investors ? 'warn' : 'neutral'}
              />
            </section>

            <SettlementReportContent detail={detail.data} />
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function SettlementReportPage(props: {
  detail: FundSettlementRunDetail | null;
  error: string | null;
  loading: boolean;
  runId: string;
}) {
  const run = props.detail?.run;

  return (
    <div className="page-stack">
      <section className="page-hero compact">
        <div>
          <p className="section-label">Settlement report</p>
          <h1>{run ? run.fund_id : 'Fund settlement'}</h1>
          <p>{run ? run.status + ' · ' + formatDate(run.status_updated_at ?? run.created_at) : props.loading ? 'Loading settlement report' : props.runId}</p>
        </div>
        <div className="flex items-center gap-2">
          {run ? <Link className="secondary-link" to={fundDetailPath(run.fund_id)}>Back to fund</Link> : null}
          {run ? (
            <Button variant="outline" type="button" render={<a href={settlementRunExportPath(run.id)} />}>
              <Download data-icon="inline-start" />
              CSV
            </Button>
          ) : null}
        </div>
      </section>

      <InlineError message={props.error} />
      {run ? (
        <>
          <section className="metrics-grid compact" aria-label="Settlement report summary">
            <Metric label="Status" value={run.status} />
            <Metric label="Basis" value={settlementBasisLabel(run.basis_source)} />
            <Metric label="Equity" value={formatNumber(run.equity)} />
            <Metric label="Investors" value={run.investor_count.toString()} />
            <Metric label="Net equity" value={formatNumber(props.detail?.totals.net_equity ?? 0)} />
            <Metric label="Tax" value={formatNumber(run.total_tax)} />
            <Metric label="Rebate" value={formatNumber(run.total_referrer_rebate)} />
            <Metric label="Capped cash" value={formatNumber(run.capped_cash_amount)} />
          </section>
          <SettlementReportContent detail={props.detail} />
        </>
      ) : null}
    </div>
  );
}

function SettlementReportContent(props: { detail: FundSettlementRunDetail | null }) {
  return (
    <>
      <section>
        <PanelTitle label="Settlement report" title="Settlement summary" action={props.detail?.run.status} />
        <DataTable
          empty="No settlement summary is available for this run."
          headers={['Line item', 'Amount']}
          rows={settlementReportRows(props.detail).map((row) => [
            row.label,
            <Value key={row.label} value={row.amount} />,
          ])}
        />
      </section>

      <section>
        <PanelTitle label="Run detail" title="Tax payable" action={(props.detail?.investor_taxes.length ?? 0) + ' investors'} />
        <DataTable
          empty="No investor tax is recorded for this run."
          headers={['Investor', 'Tax']}
          rows={(props.detail?.investor_taxes ?? []).map((tax) => [
            tax.investor,
            formatNumber(tax.tax),
          ])}
        />
      </section>

      <section>
        <PanelTitle label="Run detail" title="Referrer rebates" action={(props.detail?.referrer_rebates.length ?? 0) + ' referrers'} />
        <DataTable
          empty="No referrer rebate is recorded for this run."
          headers={['Referrer', 'Rebate']}
          rows={(props.detail?.referrer_rebates ?? []).map((rebate) => [
            rebate.referrer,
            formatNumber(rebate.rebate),
          ])}
        />
      </section>

      <section>
        <PanelTitle label="Run detail" title="Investor allocation" action={(props.detail?.investors.length ?? 0) + ' investors'} />
        <DataTable
          empty="No investor rows are recorded for this run."
          headers={['Investor', 'Referrer', 'Deposit', 'Capped cash', 'Ownership', 'Gross equity', 'Profit', 'Tax', 'Rebate', 'Net equity']}
          rows={(props.detail?.investors ?? []).map((investor) => [
            investor.name,
            investor.referrer ?? '-',
            formatNumber(investor.deposit),
            formatNumber(investor.capped_cash_amount),
            formatPercent(investor.ownership),
            formatNumber(investor.gross_equity),
            <Value key="profit" value={investor.profit} />,
            formatNumber(investor.tax),
            formatNumber(investor.referrer_rebate),
            formatNumber(investor.net_equity),
          ])}
        />
      </section>
    </>
  );
}

function FundDetailEmpty(props: { title: string; message: string }) {
  return (
    <section className="panel empty-detail">
      <p className="section-label">Fund detail</p>
      <h2>{props.title}</h2>
      <p className="muted">{props.message}</p>
      <Link className="secondary-link" to="/funds">Open Funds</Link>
    </section>
  );
}

function FundLink(props: { fund: FundConfig }) {
  return (
    <Link className="account-id-link" to={fundDetailPath(props.fund.id)}>
      {props.fund.name}
    </Link>
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
      <form className="credential-form dialog-form" onSubmit={handleSubmit}>
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
          headers={['Position', 'Product', 'Base', 'Quote', 'Settle', 'Side', 'Volume', 'Free', 'Entry', 'Mark', 'Equity', 'Exposure', 'P/L']}
          rows={props.positions.map((item) => [
            item.position_id,
            <code key="product">{item.product_id}</code>,
            item.base_currency ?? '-',
            item.quote_currency ?? '-',
            settlementCurrency(item),
            item.direction ? <Badge key="direction">{item.direction}</Badge> : 'Asset',
            formatNumber(item.volume),
            formatNumber(item.free_volume),
            formatNumber(item.position_price),
            formatNumber(item.closable_price),
            formatPositionEquity(item),
            formatPositionExposure(item),
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

function settlementBasisLabel(source: string) {
  if (source === 'live_nav') {
    return 'Live NAV';
  }
  if (source === 'legacy_statement') {
    return 'Legacy statement';
  }
  return source;
}

function settlementReportRows(detail: FundSettlementRunDetail | null) {
  if (!detail) {
    return [];
  }
  return [
    { label: 'Investor net equity', amount: detail.totals.net_equity },
    { label: 'Investor tax payable', amount: detail.totals.tax },
    { label: 'Referrer rebates payable', amount: detail.totals.referrer_rebate },
    { label: 'Retained tax', amount: detail.totals.retained_tax },
    { label: 'Capped cash audit', amount: detail.totals.capped_cash_amount },
    { label: 'Gross equity control', amount: detail.totals.gross_equity },
  ];
}

function cashFlowDirectionLabel(direction: string) {
  if (direction === 'inflow') {
    return 'Inflow';
  }
  if (direction === 'outflow') {
    return 'Outflow';
  }
  return direction;
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

function fundDetailPath(fundId: string) {
  return `/funds/detail?fund_id=${encodeURIComponent(fundId)}`;
}

function settlementReportPath(runId: string) {
  return `/funds/settlement?run_id=${encodeURIComponent(runId)}`;
}

function settlementRunExportPath(runId: string) {
  return `/api/fund-settlement-runs/export?run_id=${encodeURIComponent(runId)}`;
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
      addCurrencyTotal(summary.equityByCurrency, settlementCurrency(item), equityValue(item));
      addCurrencyTotal(summary.exposureByCurrency, exposureCurrency(item), exposureValue(item));
      return {
        total: summary.total + 1,
        assets: summary.assets + (item.direction ? 0 : 1),
        long: summary.long + (item.direction === 'LONG' ? 1 : 0),
        short: summary.short + (item.direction === 'SHORT' ? 1 : 0),
        equityByCurrency: summary.equityByCurrency,
        exposureByCurrency: summary.exposureByCurrency,
        pnl: summary.pnl + item.floating_profit,
      };
    },
    {
      total: 0,
      assets: 0,
      long: 0,
      short: 0,
      equityByCurrency: new Map<string, number>(),
      exposureByCurrency: new Map<string, number>(),
      pnl: 0,
    },
  );
}

function summarizeAccountAssets(positions: Position[]) {
  const rows = new Map<
    string,
    {
      equityValue: number;
      exposureCurrency: string;
      exposureValue: number;
      freeVolume: number;
      pnl: number;
      productId: string;
      rows: number;
      settlementCurrency: string;
      volume: number;
    }
  >();
  for (const position of positions) {
    const settle = settlementCurrency(position);
    const exposure = exposureCurrency(position);
    const rowKey = `${position.product_id}\u0000${settle}\u0000${exposure}`;
    const current = rows.get(rowKey) ?? {
      equityValue: 0,
      exposureCurrency: exposure,
      exposureValue: 0,
      freeVolume: 0,
      pnl: 0,
      productId: position.product_id,
      rows: 0,
      settlementCurrency: settle,
      volume: 0,
    };
    current.rows += 1;
    current.volume += finiteNumber(position.volume);
    current.freeVolume += finiteNumber(position.free_volume);
    current.equityValue += finiteNumber(equityValue(position));
    current.exposureValue += finiteNumber(exposureValue(position));
    current.pnl += finiteNumber(position.floating_profit);
    rows.set(rowKey, current);
  }

  return Array.from(rows.values())
    .sort((a, b) => Math.abs(b.exposureValue) - Math.abs(a.exposureValue));
}

function equityValue(position: Position) {
  return finiteNumber(position.valuation ?? position.notional_value);
}

function exposureValue(position: Position) {
  if (position.notional != null) {
    return finiteNumber(Number(position.notional));
  }

  return finiteNumber(position.notional_value);
}

function settlementCurrency(position: Position) {
  return position.settlement_currency ?? position.notional_currency ?? position.quote_currency ?? 'UNKNOWN';
}

function exposureCurrency(position: Position) {
  return position.notional_currency ?? position.quote_currency ?? 'UNKNOWN';
}

function formatPositionEquity(position: Position) {
  return `${formatNumber(equityValue(position))} ${settlementCurrency(position)}`.trim();
}

function formatPositionExposure(position: Position) {
  return `${formatNumber(exposureValue(position))} ${exposureCurrency(position)}`.trim();
}

function addCurrencyTotal(totals: Map<string, number>, currency: string | null, value: number) {
  const key = currency ?? 'UNKNOWN';
  totals.set(key, (totals.get(key) ?? 0) + finiteNumber(value));
}

function formatCurrencyBreakdown(totals: Map<string, number>) {
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

function formatPercent(value: number | null) {
  return value === null ? '-' : Intl.NumberFormat(undefined, { maximumFractionDigits: 4, style: 'percent' }).format(value);
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
