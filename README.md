# Solana HFT Bot – Yellowstone gRPC Edition

_Algorithmic market-making & high-frequency trading on Solana, written in Rust._

[![Rust](https://img.shields.io/badge/Rust-1.74%2B-orange)](https://www.rust-lang.org)  [![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## Table of Contents

1. [Project Overview](#project-overview)
2. [Features](#features)
3. [Architecture](#architecture)
4. [Quick Start](#quick-start)
5. [Configuration `bot.toml`](#configuration-bottoml)
6. [Security & Secrets](#security--secrets)
7. [Machine-Learning Pipeline](#machine-learning-pipeline)
8. [Roadmap](#roadmap)
9. [Contributing](#contributing)
10. [License](#license)

---

## Project Overview

This repository hosts an **open-source HFT bot for the Solana blockchain** which:

* streams real-time market data via **Yellowstone Geyser gRPC** (account updates),
* decodes **OpenBook** `EventQueue` fills and order-book slabs natively in Rust,
* derives micro-structure features (price, size, bid/ask spread, depth …),
* feeds them to an online **Linfa** ML model generating trade signals, and
* executes orders through **Jupiter v6** or custom on-chain programs.

The primary goal is research & education: demonstrating how to build a latency-sensitive trading stack on Solana, from raw account updates to execution, while following best engineering practices (async, strong-typing, testing, CI, secret management).

---

## Features

✔️ TLS-secured connection with optional **X-Token** authentication (required by Triton/Yellowstone).

✔️ Efficient binary decoding of `Fill` events and **best-bid/ask extraction** from OpenBook slabs.

✔️ Modular **configuration** via `bot.toml` (cluster, keypair, API keys, risk limits).

✔️ Automatic **spread** computation and streaming as Rust `TradeMsg` for downstream consumers.

✔️ Pluggable strategy layer (`strategy.rs`) and ML model (`linfa` logistic regression by default).

✔️ Works cross-platform (**Windows native**, WSL, Linux, macOS).

---

## Architecture

```
┌────────────────────────────┐      ┌────────────────────────────┐
│  Yellowstone Geyser gRPC   │      │  OpenBook  Market Accounts │
└────────────┬───────────────┘      └────────────┬───────────────┘
             │                                       │
       AccountUpdateStream                          │
             │                                       │
             ▼                                       ▼
 ┌──────────────────┐     async mpsc    ┌──────────────────┐
 │  grpc_stream.rs  ├──────────────────▶│   trader.rs      │
 │  (decoding)      │                   │   (strategy)     │
 └──────────────────┘                   └─▲───────────────▲┘
             │                             │               │
             ▼                             │ Linfa         │
     TradeMsg { price, size, spread }      │ predictions   │
                                           │               │
                                 ┌─────────┴─────────┐     │
                                 │ Execution layer   │◀────┘
                                 │ (Jupiter / CLOB)  │
                                 └────────────────────┘
```

---

## Quick Start

### Prerequisites

* **Rust** ≥ 1.74 (`rustup default stable`)
* 64-bit OS (Windows, macOS, Linux)
* A Solana wallet/keypair with enough lamports for fees
* A [Yellowstone](https://triton.one/)
  or [Triton](https://triton.one/) API token _(optional for public endpoint)_

### Build & Run (paper-trading)

```bash
# clone
git clone https://github.com/Sitia8/botsolana.git
cd botsolana

# install Rust toolchain + components
rustup component add rustfmt clippy

# create config from template
cp bot.example.toml bot.toml && $EDITOR bot.toml

# compile & run
cargo run --release -- --config bot.toml
```

---

## Configuration `bot.toml`

```toml
# --- Cluster & Accounts -----------------------------------------------------
cluster          = "https://api.mainnet-beta.solana.com"
wallet_keypair   = "<PATH/OR/BASE58>"          # keep secret 🔒

# --- Yellowstone / Triton ---------------------------------------------------
yellowstone_token = "<OPTIONAL_X_TOKEN>"        # leave blank for public

# --- Trading ---------------------------------------------------------------
base_symbol      = "SOL"                        # only SOL/USDC supported for now
quote_symbol     = "USDC"
max_position     = 10.0                         # SOL
order_size       = 0.2                          # SOL per order

# --- Machine-Learning ------------------------------------------------------
model_path       = "model.bin"                  # generated by training script
```

> **Never** commit `bot.toml` — see [.gitignore](./.gitignore).

A commented template (`bot.example.toml`) is provided for convenience.

---

## Security & Secrets

1. **Private keys** and API tokens live only in `bot.toml` or environment variables.
2. `.gitignore` prevents accidental commits.
3. Consider [1Password Secrets Automation](https://developer.1password.com/docs/cli) or similar for production.

---

## Machine-Learning Pipeline

The default strategy trains an online **logistic regression** (Linfa) on three micro-structure features: `price`, `size`, `spread`.

A separate binary `train_model.rs` (WIP) ingests historical fills (Parquet/CSV) and outputs a `model.bin` compatible with the runtime.

Feel free to replace it with gradient-boosted trees, transformers, etc.

---

## Roadmap

- [ ] Full slab parser (`openbook_dex::state::Slab`) for depth & imbalance metrics.
- [ ] Unit & integration tests (CI – GitHub Actions).
- [ ] Kubernetes-ready Dockerfile.
- [ ] Multi-asset support (per-market tasks).

---

## Contributing

PRs are welcome! Please run `cargo fmt && cargo clippy --all-targets -- -D warnings` before submitting.

---

## License

Licensed under the MIT license. See [LICENSE](LICENSE) for details.

---

_The following legacy README is kept below for historical reference._

---

