# 1Exchange

1Exchange 是一个单用户本地运行的多交易所统一账户应用。项目参考 `~/Projects/Yuan` 中的多交易所适配模型，迁移为 Rust 版本，用一套本地服务统一管理交易所 Credential、拉取账户资产和持仓，并提供 HTTP API 与 Web UI。

## 目标

- 对接多个交易所，隐藏各交易所 API 差异。
- 在本地管理 Credential，不依赖外部托管服务。
- 汇总账户资产、持仓、未完成订单等信息。
- 使用 SQLite 存储本地数据，默认目录为 `~/.1ex/`。
- 提供 HTTP API，默认假设单用户本机使用，不做 Auth。
- 提供 Web UI，使用 Vite + TypeScript + React。
- 通过 Rust Server 反向代理或托管前端静态资源，实现一键启动。

## 参考模型

核心数据模型参考 Yuan：

- `AccountInfo`：账户快照，包含 `positions`、`orders` 和生成时间；不迁移已废弃的 `IAccountMoney`。
- `Position`：账户下的原子资产/持仓，支持按品种和方向聚合。
- `Product`：交易品种规格，保留交易所原始 `product_id`，避免过早做统一命名。
- `Product.volume_step`：已乘入交易所合约乘数；`Product.value_scale` 统一为 `1`，避免调用方记忆“一手”等于多少价值单位。
- Exchange Adapter：每个交易所实现 Credential Schema、产品列表、持仓、订单等能力。

Rust 版本会保留这些语义，但使用 Rust 类型、SQLite 表结构和 HTTP JSON API 表达。

账户持仓响应对齐 1Earn 当前使用的 Yuants IPosition 字段：除原有 volume、free_volume、notional_value 外，也返回 account_id、datasource_id、size、free_size、valuation 以及资金结算相关可选字段。notional_value 作为 1Exchange 旧字段保留，valuation 作为 1Earn/Yuants 标准字段供组合账户、配对模式和外部来源消费。

资产结构固定为：

```text
AccountInfo（账户） -> Position（持仓 / 资产） -> Product（规格）
```

`Position.product_id` 关联 `Product.product_id`。账户资产视图来自 `AccountInfo.positions`，不再引入独立的 `IAccountMoney` 或 `AssetStat` 模型。

## 架构

```text
1Exchange
├── Rust Server
│   ├── HTTP API
│   ├── SQLite Repository
│   ├── Credential Manager
│   ├── Exchange Adapter Registry
│   └── Static Web UI Service
└── Web UI
    ├── Vite
    ├── TypeScript
    └── React
```

## 本地数据

默认数据目录：

```text
~/.1ex/
```

默认数据库文件：

```text
~/.1ex/1ex.sqlite3
```

后续可以通过环境变量覆盖数据目录和监听地址。

## HTTP API

当前已建立以下 API 边界：

- `GET /api/health`：服务健康检查。
- `GET /api/exchanges`：列出已支持交易所。
- `GET /api/credentials`：列出本地 Credential 元信息。
- `POST /api/credentials`：新增 Credential。
- `GET /api/custom-account-sources`：列出自定义账户来源。
- `POST /api/custom-account-sources`：注册一个 1Exchange-compatible BASE URL 作为账户来源。
- `GET /api/accounts`：发现并列出所有账户，包括本地 Credential、Virtual Account 和自定义来源账户。
- `GET /api/accounts?credential_id=...`：按本地 Credential 拉取账户快照。
- `GET /api/accounts?account_id=...`：按 AccountID 拉取账户快照；本地真实账户、Virtual Account 和自定义来源账户使用同一接口。
- `GET /api/virtual-accounts`：列出本地虚拟账户配置。
- `POST /api/virtual-accounts`：创建或更新虚拟账户配置。
- `GET /api/positions?credential_id=...`：按本地 Credential 拉取账户持仓/资产投影。
- `GET /api/positions?account_id=...`：按 AccountID 拉取账户持仓/资产投影；本地真实账户、Virtual Account 和自定义来源账户使用同一接口。
- `GET /api/trades?credential_id=...`：按本地 Credential 拉取最近一批历史成交流水。
- `GET /api/rates?target=USD`：返回当前汇率图快照，汇率边可多跳换算到目标币种。
- `GET /api/rates/convert?from=USDC&to=USD`：检查单个币种到目标币种的当前换算路径结果。
- `GET /api/products?exchange=BINANCE`：列出指定交易所的交易产品规格。

当前 `accounts`、`positions`、`products` 已固定标准响应模型，但真实数据拉取仍待交易所 Adapter 接入。

## Custom Account Source

Custom Account Source 允许注册一个 `BASE URL`，让当前实例从远端发现并读取账户。远端可以是另一个 1Exchange 实例，也可以是任何实现了 1Exchange account API 子集的服务。当前需要支持：

- `GET /api/accounts`：返回 `AccountInfo[]`，用于发现账户。
- `GET /api/accounts?account_id=...`：返回单个 AccountID 对应的 `AccountInfo[]`。

当前实例不会轮询远端来源；只有调用 `GET /api/accounts`、`GET /api/accounts?account_id=...` 或 `GET /api/positions?account_id=...` 时才临时请求远端。

## Virtual Account

Virtual Account 参考 Yuan Account Composer 的线性组合语义，但在 1Exchange 中是本地按需查询：配置持久化在 SQLite，服务不会订阅、发布或轮询来源账户。Virtual Account 读取与普通 Account 一视同仁，使用 `GET /api/accounts?account_id=...` 或 `GET /api/positions?account_id=...`。调用这些接口或在 GUI 点击 `Compose now` 时，服务才会临时读取各来源 credential 的账户快照，按系数缩放后合并为一个新的 `AccountInfo`。

每个 source 使用 `coefficient` 表达线性运算：`1` 表示加入该账户，`-1` 表示扣减该账户，`2` 表示乘以二，`0.5` 表示除以二。`force_zero` 会保留来源持仓行，但把 volume、free volume、notional 和 floating profit 强制缩放为 0，适合构造只用于平仓/对齐的虚拟账户视图。

## 汇率图

汇率不是一张“所有币直接到 USD”的表，而是一张有向图：每条边表示 `base_currency -> quote_currency` 的当前换算率。Portfolio 做 USD 估值时会在图上查找路径，例如 `OKSOL -> USDC -> USD`。找不到路径的币种不会被强行估值，会在 GUI 中显示为 `unpriced`。

当前第一版只内置稳定币锚点边：`USD`、`USDT`、`USDC`、`USDD` 之间按 `1:1` 互相连通。后续汇率更新建议按以下顺序接入：

- 公共 ticker 边：从已支持交易所的公开行情拉取 `BASE/QUOTE` mid price，不使用私有 credential。
- SQLite 缓存：保存 `base_currency`、`quote_currency`、`rate`、`source`、`updated_at`、`ttl`，服务启动后先读缓存，再后台刷新。
- 多来源合并：同一条边可以保留多个来源，默认使用最新且未过期的 rate；必要时再加 source priority。
- 更新节奏：稳定币锚点为静态边；主流公共 ticker 可 30-60 秒刷新；低频或失败的边保留旧值直到 TTL 过期。
- 安全语义：没有路径就是未知估值，不显示为 0。

当前实质接入状态：

| 交易所 ID | Products | Account / Position |
| --- | --- | --- |
| `BINANCE` | 已接入公开现货和 U 本位合约产品 | 已接入 Spot 余额、USD-M Futures 资产和持仓 |
| `OKX` | 已接入公开现货、杠杆和永续产品 | 已接入 Trading 余额和持仓、Funding 资产、Savings 资产、Flexible Loan 资产和负债；已接入最近 SPOT/SWAP 成交流水 |
| `HTX` | 已接入公开现货和 U 本位合约产品 | 已接入 Spot 只读余额、U 本位合约账户模式识别、联合保证金资产和持仓；已接入最近 U 本位合约成交流水 |
| `GATE` | 已接入公开现货和 U 本位合约产品 | 已接入 Spot 余额、USDT 永续持仓、Unified 资产和 Earning 资产；已接入最近 USDT 永续成交流水 |
| `BITGET` | 已接入公开现货、U 本位和币本位合约产品 | 已接入 UTA v3 账户资产、USDT-FUTURES 和 COIN-FUTURES 持仓；已接入最近 SPOT/USDT-FUTURES/COIN-FUTURES 成交流水 |
| `HYPERLIQUID` | 已接入公开现货和永续产品 | 已接入只读账户和持仓；已接入最近用户 fills |
| `ASTER` | 已接入公开现货和永续产品 | 已接入 Pro API V3 永续账户和持仓；Spot V3 私有账户官方接口当前返回 500，暂未纳入主链路 |

当前交易所注册表覆盖：

| 交易所 ID | 名称 | Credential 必填字段 |
| --- | --- | --- |
| `BINANCE` | Binance | `access_key`, `secret_key` |
| `OKX` | OKX | `access_key`, `secret_key`, `passphrase` |
| `HTX` | HTX | `access_key`, `secret_key` |
| `GATE` | Gate.io | `access_key`, `secret_key` |
| `BITGET` | Bitget | `access_key`, `secret_key`, `passphrase` |
| `HYPERLIQUID` | HyperLiquid | `address` |
| `ASTER` | Aster | `address`, `signer`, `private_key` |

`POST /api/credentials` 会校验对应交易所的必填字段必须存在且为非空字符串。

Credential 创建请求示例：

```json
{
  "exchange": "BINANCE",
  "name": "main",
  "payload": {
    "access_key": "...",
    "secret_key": "..."
  }
}
```

Credential 列表只返回元信息和 `has_payload`，不会回传密钥内容。

接口不做 Auth。安全边界是本机单用户运行和本地网络访问控制。

## 开发状态

当前阶段：迁移标准账户模型、产品模型、Credential 管理和交易所 Adapter 边界。

已完成：

1. 建立 Rust HTTP Server 骨架。
2. 建立 SQLite 数据层和基础迁移。
3. 建立 Yuan 语义对应的账户、持仓、订单、产品模型，并排除已废弃的 `IAccountMoney`。
4. 建立 Credential SQLite 存储和 HTTP 创建、列表接口。
5. 建立 Exchange Adapter trait 与首批交易所注册信息。
6. 建立 Vite + TS + React Web UI。
7. 将前端构建产物接入 Rust Server 托管。

下一步：

1. 继续补齐 HTX super-margin、ASTER Spot 私有资产等剩余只读 Positions 来源。
2. 增加 Credential 更新、删除和本地加密策略。
3. 在 Web UI 接入 Credential、Products、Accounts、Positions 页面。

## 启动方式

开发期后端启动：

```bash
npm --prefix web install
cargo run
```

Debug 模式下，Rust Server 会自动启动 Vite Dev Server：

- Rust API 默认地址：`http://127.0.0.1:8787`。
- 未设置 `ONE_EXCHANGE_VITE_ADDR` 时，Vite UI 会自动选择可用端口。
- Vite UI 实际地址会在启动日志中打印。
- Vite 会把 `/api` 代理到当前 Rust API 地址。

可用环境变量：

- `ONE_EXCHANGE_ADDR=127.0.0.1:8787`：覆盖 Rust API 监听地址。
- `ONE_EXCHANGE_VITE_ADDR=127.0.0.1:5173`：固定 Vite Dev Server 地址。
- `ONE_EXCHANGE_VITE=0`：关闭自动启动 Vite。

本地检查：

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

如需单独启动前端开发服务：

```bash
npm --prefix web install
npm --prefix web run dev
```

生产期目标是一条命令启动 Rust Server，并由 Rust Server 托管 `web/dist` 静态资源。
