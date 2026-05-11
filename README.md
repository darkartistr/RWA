# RWA Tokenization Service (Solana)

В этом репозитории я реализовал микросервис для токенизации RWA в сети Solana.  
Сервис поддерживает базовые операции с токенами:

- `mint` (выпуск),
- `transfer` (перевод),
- `burn` (сжигание).

Я сделал два интерфейса доступа:

- REST API (HTTP/JSON),
- gRPC.

Также в проекте есть офчейн-инфраструктура:

- PostgreSQL для хранения метаданных и истории операций,
- Liquibase для версионности схемы БД,
- Kafka + Zookeeper для событий,
- Prometheus + Grafana для мониторинга.

---

## Как устроен проект

Ниже я описываю структуру так, как сам в ней ориентируюсь.

### Корень репозитория

- `Cargo.toml` — workspace-файл Rust, в котором я подключил основные crate:
  - `crates/service`,
  - `crates/proto`,
  - `crates/migrations`.
- `docker-compose.yml` — локальная инфраструктура (Postgres, Kafka, Zookeeper, Prometheus, Grafana, Liquibase).
- `.env.example` — пример переменных окружения для сервиса.
- `README.md` — этот файл.

### `crates/service` — основной off-chain микросервис

Это главный код приложения.

- `crates/service/Cargo.toml` — зависимости сервиса (`axum`, `tonic`, `sqlx`, `rdkafka`, `solana-sdk`, `solana-client`, `serde`, `prometheus` и т.д.).
- `crates/service/src/main.rs` — точка входа:
  - читаю конфиг из env,
  - поднимаю HTTP и gRPC серверы,
  - инициализирую Solana-клиент, PostgreSQL, Kafka и метрики.
- `crates/service/src/config.rs` — загрузка переменных окружения в структуру `Config`.
- `crates/service/src/api.rs` — REST API:
  - `POST /v1/mints`,
  - `POST /v1/mint_to`,
  - `POST /v1/transfer`,
  - `POST /v1/burn`,
  - `GET /healthz`,
  - `GET /metrics`.
- `crates/service/src/grpc.rs` — gRPC-обработчики тех же операций (`CreateMint`, `MintTo`, `Transfer`, `Burn`).
- `crates/service/src/solana.rs` — работа с Solana RPC и SPL Token:
  - создание mint,
  - mint_to,
  - transfer,
  - burn,
  - подпись транзакций через keypair authority.
- `crates/service/src/db.rs` — подключение к PostgreSQL и доступ к `PgPool`.
- `crates/service/src/kafka.rs` — отправка JSON-событий в Kafka topic.
- `crates/service/src/metrics.rs` — регистрация и экспорт метрик Prometheus.

### `crates/proto` — gRPC контракт

- `crates/proto/proto/token_service.proto` — описание gRPC API и сообщений.
- `crates/proto/build.rs` — генерация Rust-кода из `.proto` через `tonic-build`.
- `crates/proto/src/lib.rs` — подключение сгенерированного модуля.

### `crates/migrations` — миграции БД

- `crates/migrations/liquibase/db.changelog.xml` — schema-миграции Liquibase:
  - таблица `token_mints`,
  - таблица `token_operations`,
  - индексы для поиска по `mint` и `signature`.
- `crates/migrations/src/lib.rs` — пустой crate-заглушка под workspace (миграции запускаются Liquibase-контейнером).

### `onchain` — смарт-контракт (Anchor)

- `onchain/Anchor.toml` — Anchor-конфигурация workspace/program.
- `onchain/programs/rwa_asset_registry/Cargo.toml` — зависимости on-chain программы (`anchor-lang`).
- `onchain/programs/rwa_asset_registry/src/lib.rs` — код Anchor-программы:
  - `initialize_asset`,
  - `set_admin`,
  - структуры аккаунтов и PDA-seeds.

### `ops` — мониторинг

- `ops/prometheus/prometheus.yml` — scrape-конфиг Prometheus (`/metrics` сервиса).
- `ops/grafana/provisioning/datasources/datasource.yml` — datasource Prometheus для Grafana.
- `ops/grafana/provisioning/dashboards/dashboards.yml` — автоподключение дашбордов.
- `ops/grafana/provisioning/dashboards/json/rwa-service.json` — готовый dashboard для метрик сервиса.

---

## Поток данных

1. Клиент вызывает REST или gRPC метод.
2. Я валидирую входные данные и формирую Solana-транзакцию.
3. Транзакция подписывается authority keypair и отправляется через RPC.
4. Результат операции пишется в PostgreSQL (`token_operations` / `token_mints`).
5. Событие отправляется в Kafka (JSON).
6. Метрики обновляются и отдаются в Prometheus, после чего видны в Grafana.

---

## Быстрый запуск (dev)

1) Поднять инфраструктуру:

```bash
docker compose up -d
```

2) Создать локальный `.env`:

```bash
cp .env.example .env
```

3) Запустить сервис:

```bash
cargo run -p rwa-service
```

---

## Переменные окружения (основные)

- `HTTP_ADDR` — адрес REST API (по умолчанию `0.0.0.0:8080`),
- `GRPC_ADDR` — адрес gRPC API (по умолчанию `0.0.0.0:9090`),
- `SOLANA_RPC_URL` — RPC endpoint Solana,
- `SOLANA_COMMITMENT` — уровень commitment,
- `SOLANA_KEYPAIR_PATH` — путь к authority keypair JSON,
- `DATABASE_URL` — строка подключения к PostgreSQL,
- `KAFKA_BROKERS` — адрес брокеров Kafka,
- `KAFKA_TOPIC_TOKEN_EVENTS` — топик для событий.

---

## Что смотреть в первую очередь

Если я возвращаюсь в этот проект после паузы, то иду в таком порядке:

1. `crates/service/src/main.rs` — как сервис собирается и запускается.
2. `crates/service/src/api.rs` и `crates/service/src/grpc.rs` — внешний контракт и обработка запросов.
3. `crates/service/src/solana.rs` — Solana-транзакции и подпись.
4. `crates/migrations/liquibase/db.changelog.xml` — структура БД.
5. `docker-compose.yml` — как локально поднимается вся инфраструктура.

---

## Полная структура проекта

```text
RWA/
├─ Cargo.toml                              # Rust workspace (общий список crate)
├─ docker-compose.yml                      # локальная инфраструктура (Postgres/Kafka/Prometheus/Grafana/Liquibase)
├─ .env.example                            # пример env-переменных
├─ .gitignore                              # git-исключения
├─ README.md                               # документация проекта
│
├─ crates/
│  ├─ service/                             # основной микросервис (off-chain логика)
│  │  ├─ Cargo.toml                        # зависимости сервиса: axum/tonic/sqlx/rdkafka/solana-sdk/prometheus/serde
│  │  └─ src/
│  │     ├─ main.rs                        # entrypoint приложения
│  │     ├─ config.rs                      # конфигурация через env
│  │     ├─ api.rs                         # REST API (JSON)
│  │     ├─ grpc.rs                        # gRPC handlers
│  │     ├─ solana.rs                      # Solana RPC + SPL Token операции
│  │     ├─ db.rs                          # PostgreSQL (sqlx pool)
│  │     ├─ kafka.rs                       # Kafka producer (события)
│  │     └─ metrics.rs                     # Prometheus метрики
│  │
│  ├─ proto/                               # контракт gRPC
│  │  ├─ Cargo.toml                        # зависимости proto crate (tonic/prost)
│  │  ├─ build.rs                          # генерация кода из .proto
│  │  ├─ src/lib.rs                        # include сгенерированного proto-модуля
│  │  └─ proto/token_service.proto         # описание gRPC API
│  │
│  └─ migrations/                          # миграции схемы базы данных
│     ├─ Cargo.toml                        # crate-заглушка для workspace
│     ├─ src/lib.rs                        # пустой файл-заглушка
│     └─ liquibase/db.changelog.xml        # Liquibase changelog
│
├─ onchain/
│  ├─ Anchor.toml                          # Anchor workspace config
│  └─ programs/
│     └─ rwa_asset_registry/
│        ├─ Cargo.toml                     # on-chain зависимости (anchor-lang)
│        └─ src/lib.rs                     # Anchor программа (asset registry)
│
└─ ops/
   ├─ prometheus/prometheus.yml            # конфиг scrape-целей
   └─ grafana/provisioning/
      ├─ datasources/datasource.yml        # datasource Prometheus
      └─ dashboards/
         ├─ dashboards.yml                 # auto-provisioning dashboard
         └─ json/rwa-service.json          # JSON дашборд сервиса
```