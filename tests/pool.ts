import * as anchor from "@project-serum/anchor";
import { Program, web3, BN, Spl } from "@project-serum/anchor";
import {PublicKey, Keypair} from '@solana/web3.js';
import { Flashloan } from "../target/types/flashloan";

import { expect } from 'chai';
import * as chai from 'chai';
import chaiAsPromised from 'chai-as-promised';
chai.use(chaiAsPromised);

describe("flashloan", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());
  const provider = anchor.getProvider();
  const program = anchor.workspace.Flashloan as Program<Flashloan>;
  const spl_token = Spl.token();

  const flashloan = Keypair.generate();
  const authority = Keypair.generate();

  const mint = Keypair.generate();
  const token1 = Keypair.generate();
  const lp_token1 = Keypair.generate();

  async function create_mint(mint: Keypair, mint_authority: PublicKey) {
    await spl_token.methods
        .initializeMint(9, mint_authority, null)
        .accounts({
          mint: mint.publicKey,
          rent: web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([mint])
        .preInstructions([await spl_token.account.mint.createInstruction(mint)])
        .rpc();
  }

  async function create_token(token: Keypair, mint: PublicKey, authority: PublicKey) {
    await spl_token.methods.initializeAccount()
        .accounts({
          account: token.publicKey,
          mint: mint,
          authority: authority,
          rent: web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([token])
        .preInstructions([await spl_token.account.token.createInstruction(token)])
        .rpc();
  }

  async function find_token_authority(flashloan: PublicKey) {
    return await PublicKey.findProgramAddress(
      [flashloan.toBuffer(),
        Buffer.from(anchor.utils.bytes.utf8.encode("flashloan"))],
      program.programId
    );
  }

  async function find_pool(flashloan: PublicKey, mint: PublicKey) {
    const [pool, _nonce] = await PublicKey.findProgramAddress(
      [flashloan.toBuffer(), mint.toBuffer()],
      program.programId
    );

    return pool;
  }

  async function find_pool_token(flashloan: PublicKey, mint: PublicKey) {
    const [pool_token, _nonce] = await PublicKey.findProgramAddress(
      [flashloan.toBuffer(), Buffer.from(anchor.utils.bytes.utf8.encode("token")), mint.toBuffer()],
      program.programId
    );

    return pool_token;
  }

  async function find_lp_token_mint(flashloan: PublicKey, mint: PublicKey) {
    const [lp_token_mint, _nonce] = await PublicKey.findProgramAddress(
      [flashloan.toBuffer(), Buffer.from(anchor.utils.bytes.utf8.encode("liquidity_pool")), mint.toBuffer()],
      program.programId
    );

    return lp_token_mint;
  }

  before(async () => {
    await create_mint(mint, provider.wallet.publicKey);
    await create_token(token1, mint.publicKey, provider.wallet.publicKey);
  })

  it("Should initialize flashloan", async () => {
    await program.methods
      .initialize()
      .accounts({
        flashloan: flashloan.publicKey,
        authority: authority.publicKey,
      })
      .signers([flashloan])
      .rpc();

    const flashloanAccount = await program.account.flashLoan.fetch(flashloan.publicKey);
    expect(flashloanAccount.authority).to.be.deep.equal(authority.publicKey);

    const [token_authority, bump] = await find_token_authority(flashloan.publicKey);
    expect(flashloanAccount.tokenAuthorityBump).to.be.equal(bump);
  });

  it("Should add pool", async () => {
    await program.methods
      .addPool()
      .accounts({
        flashloan: flashloan.publicKey,
        authority: authority.publicKey,
        tokenMint: mint.publicKey,
      })
      .signers([authority])
      .rpc();

    const poolAccount = await program.account.pool.fetch(await find_pool(flashloan.publicKey, mint.publicKey));
    const poolTokenAccount = await spl_token.account.token.fetch(await find_pool_token(flashloan.publicKey, mint.publicKey));
    const lpTokenMintAccount = await spl_token.account.mint.fetch(await find_lp_token_mint(flashloan.publicKey, mint.publicKey));
    const mintAccount = await spl_token.account.mint.fetch(mint.publicKey);

    const [token_authority, _nonce] = await find_token_authority(flashloan.publicKey);

    expect(poolAccount.borrowing).to.be.false;
    expect(poolAccount.tokenMint).to.be.deep.equal(mint.publicKey);
    expect(poolAccount.poolToken).to.be.deep.equal(await find_pool_token(flashloan.publicKey, mint.publicKey));
    expect(poolAccount.lpTokenMint).to.be.deep.equal(await find_lp_token_mint(flashloan.publicKey, mint.publicKey))

    expect(poolTokenAccount.mint).to.be.deep.equal(mint.publicKey);
    expect(poolTokenAccount.authority).to.be.deep.equal(token_authority);
    expect(poolTokenAccount.amount.toNumber()).to.be.equal(0);

    expect(lpTokenMintAccount.mintAuthority).to.be.deep.equal(token_authority);
    expect(lpTokenMintAccount.supply.toNumber()).to.be.equal(0);
    expect(lpTokenMintAccount.decimals).to.be.equal(mintAccount.decimals);
  });

  it("Should add liquidity", async () => {
    await spl_token.methods
      .mintTo(new BN(1000000))
      .accounts(
        {
          mint: mint.publicKey,
          to: token1.publicKey,
          authority: provider.wallet.publicKey,
        })
      .rpc();

    const [tokenAuthority, _nonce] = await find_token_authority(flashloan.publicKey);
    const lpTokenMint = await find_lp_token_mint(flashloan.publicKey, mint.publicKey)
    await create_token(lp_token1, lpTokenMint, provider.wallet.publicKey);

    let tokenAccount = await spl_token.account.token.fetch(token1.publicKey);
    expect(tokenAccount.amount.toNumber()).to.be.equal(1000000);

    const pool = await find_pool(flashloan.publicKey, mint.publicKey);
    await program.methods
      .deposit(new BN(1000000))
      .accounts({
        flashloan: flashloan.publicKey,
        pool,
        userToken: token1.publicKey,
        userLpToken: lp_token1.publicKey,
      })
      .preInstructions(
        [
          await spl_token.methods
            .approve(new BN(1000000))
            .accounts({
              source: token1.publicKey,
              delegate: tokenAuthority,
              authority: provider.wallet.publicKey
            }).instruction()
        ]
      )
      .rpc();

    const poolAccount = await program.account.pool.fetch(pool);
    const poolTokenAccount = await spl_token.account.token.fetch(poolAccount.poolToken);
    const lpToken1Account = await spl_token.account.token.fetch(lp_token1.publicKey);

    expect(poolTokenAccount.amount.toNumber()).to.be.equal(1000000);
    expect(lpToken1Account.amount.toNumber()).to.be.equal(1000000);
  })

  it("Should remove liquidity", async () => {
    const [tokenAuthority, _nonce] = await find_token_authority(flashloan.publicKey);
    const pool = await find_pool(flashloan.publicKey, mint.publicKey);

    let lpTokenAccount = await spl_token.account.token.fetch(lp_token1.publicKey);
    let tokenAccount = await spl_token.account.token.fetch(token1.publicKey);
    expect(lpTokenAccount.amount.toNumber()).to.be.equal(1000000);
    expect(tokenAccount.amount.toNumber()).to.be.equal(0);

    await program.methods
      .withdraw(new BN(1000000))
      .accounts(
        {
          flashloan: flashloan.publicKey,
          pool,
          userToken: token1.publicKey,
          userLpToken: lp_token1.publicKey,
        })
      .preInstructions(
        [
          await spl_token.methods
            .approve(new BN(1000000))
            .accounts({
              source: lp_token1.publicKey,
              delegate: tokenAuthority,
              authority: provider.wallet.publicKey
            }).instruction()
        ]
      )
      .rpc();

    lpTokenAccount = await spl_token.account.token.fetch(lp_token1.publicKey);
    tokenAccount = await spl_token.account.token.fetch(token1.publicKey);
    expect(lpTokenAccount.amount.toNumber()).to.be.equal(0);
    expect(tokenAccount.amount.toNumber()).to.be.equal(1000000);
  });

});
