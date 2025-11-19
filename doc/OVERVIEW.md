# KRC-721 Indexer Overview


## Overview

KRC-721 is a token standard for non-fungible tokens (NFTs) on the Kaspa network. It defines a set of rules and interfaces for creating, managing, and transferring unique digital assets.

## Features

- Security budget-driven transactions (fixed PoW fees)
- Optional pre-minting during deployment
- Optional discounted minting (reduced royalty fees per address)
- Optional royalty fees
- Optional DAA start time for minting
- Metadata can be either inscribed or specified as an external URL
- Full randomization of tokens during minting
- Creator-defined number of tokens per collection.

## Restrictions

This indexer only supports IPFS CIDs for metadata and image URLs. All URLs must start with the `ipfs://` prefix. While the indexer will accept any URL during deployment without validation, only tokens with IPFS-compliant URLs will be processed. All tools that enable KRC-721 token deployment must enforce this restriction to prevent invalid deployments.

## Deployments

Indexer deployments are available for the following networks:

- `mainnet` at [https://mainnet.krc721.stream](https://mainnet.krc721.stream)
- `testnet-10` at [https://testnet-10.krc721.stream](https://testnet-10.krc721.stream)

