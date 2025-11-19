## KRC-721 - Kaspa NFT indexer

This is an archive repository created for public access.

## Features

- NFT Collection deployments with pre-minting
- NFT minting
- NFT transfers
- Minimum PoW fees for indexer operations 
- Custom-definable royalty fees for content creators (collected during mints)
- Unlimited mints
- Randomized minting token order
- Fully compatible with Testnet-11 (10+ BPS)
- High-availability load-balancing (cluster mode operation)


## Installation

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

```bash
sudo apt update && sudo apt upgrade -y
```

```bash
sudo apt install jq curl git build-essential libssl-dev pkg-config \
protobuf-compiler libprotobuf-dev \
clang-format clang-tidy \
clang-tools clang clangd libc++-dev \
libc++1 libc++abi-dev libc++abi1 \
libclang-dev libclang1 liblldb-dev \
libllvm-ocaml-dev libomp-dev libomp5 \
lld lldb llvm-dev llvm-runtime \
llvm python3-clang
```

Optionally:

```bash
sudo apt install nginx
```

Configuration files can be found in the [`doc/deployment`](`doc/deployment`) directory.

## Running KRC-721 indexer daemon

You can run via cargo or the binary directly:
```
# via cargo
cargo run --release -- --help
# daemon directly
./krc721d --help
./target/release/krc721d --help
```

IMPORTANT: Before running the indexer, you must run and synchronize the integrated Rusty Kaspa node.
Once the node is synced, stop the indexer, sync it's state with another indexer and restart it.
The sequence of these steps is important due to the fact that the Rusty Kaspa node may take long
time to synchronize, resulting in the indexer state becoming outdated.

### 1. Sync kaspa node
```bash
./krc721d --mainnet --local
```

### 2. Sync indexer state from another indexer
```bash
./krc721d --mainnet --sync=https://mainnet.krc721.stream
```

### 3. Start the indexer
```bash
./krc721d --mainnet --local --http
```

## Node Failure and Pruning

If the node fails for any reason, stopped or disconnected from the network, eventually the indexer state will become outdated.

- You can use `--retention-period-days` flag to extend the node pruning period
- You should run at least 2 instances to be able to recover one from another in case of a failure.

## Test Deployments

You should always run your own indexer in production mode. The following indexers are available for development purposes:

- https://mainnet.krc721.stream
- https://testnet-10.krc721.stream

