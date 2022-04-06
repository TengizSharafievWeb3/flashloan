# Portfolio project - Composable Solana Flash Loan 

## Motivation

Traditional EVM flash loans are based on the callback functionality.
The flash loan smart contract expects a callback smart contract as an argument, which will borrow and repay funds inside.
If the callback smart contract doesn't repay the expected amount of funds, the transaction will be failed.

It is possible to implement flash loans on the Solana in the same way,
but its functionality will be limited due to limited due to the reentrancy of Solana transactions.

But Solana allows using instruction introspection on-chain.
What it means is being able to inspect the instructions present in the transaction that is being executed, from within another instruction. This is useful because all Solana transactions are atomic, meaning all parts of a transaction need to succeed in order for the whole to succeed.

This repository contains an implementation of such an approach plus some whistles.

## Installation

- (Rust) [rustup](https://www.rust-lang.org/tools/install)
- (Solana) [solan-cli](https://docs.solana.com/cli/install-solana-cli-tools) 1.9.14
- (Anchor) [anchor](https://book.anchor-lang.com/chapter_2/installation.html) 0.23
- (Node) [node](https://github.com/nvm-sh/nvm) 17.4.0

## Build & Test

```
% anchor build
% yarn install
% anchor test
```

## Whistles

- [x] Reward fee settings
- [x] Discount voucher for repay