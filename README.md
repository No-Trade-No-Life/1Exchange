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

- `IAccountInfo`：账户快照，包含持仓、订单和生成时间；不迁移已废弃的 `IAccountMoney`。
- `IPosition`：原子持仓，支持按品种和方向聚合。
- `IProduct`：交易品种，保留交易所原始 `product_id`，避免过早做统一命名。
- Exchange Adapter：每个交易所实现 Credential Schema、产品列表、持仓、订单等能力。

Rust 版本会保留这些语义，但使用 Rust 类型、SQLite 表结构和 HTTP JSON API 表达。

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
- `GET /api/accounts`：列出账户快照。
- `GET /api/positions`：列出聚合持仓。
- `GET /api/products`：列出交易产品。

当前 `accounts`、`positions`、`products` 已固定标准响应模型，但真实数据拉取仍待交易所 Adapter 接入。资产统计会基于后续新的资产/余额模型设计，不复用 Yuan 已废弃的 `IAccountMoney`。

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

1. 为 Binance、Gate、Aster、Hyperliquid 逐个实现真实 Adapter。
2. 设计新的资产/余额模型。
3. 将账户、持仓、资产和产品 API 接入 Adapter Registry。
4. 增加 Credential 更新、删除和本地加密策略。

## 启动方式

开发期后端启动：

```bash
cargo run
```

前端开发服务：

```bash
npm --prefix web install
npm --prefix web run dev
```

生产期目标是一条命令启动 Rust Server，并由 Rust Server 托管 `web/dist` 静态资源。
