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
  const token2 = Keypair.generate();
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
    await create_token(token2, mint.publicKey, provider.wallet.publicKey);

    await spl_token.methods
      .mintTo(new BN(1001 * web3.LAMPORTS_PER_SOL))
      .accounts(
        {
          mint: mint.publicKey,
          to: token1.publicKey,
          authority: provider.wallet.publicKey,
        })
      .rpc();

    await program.methods
      .initialize(10)
      .accounts({
        flashloan: flashloan.publicKey,
        authority: authority.publicKey,
      })
      .signers([flashloan])
      .rpc();

    await program.methods
      .addPool()
      .accounts({
        flashloan: flashloan.publicKey,
        authority: authority.publicKey,
        tokenMint: mint.publicKey,
      })
      .signers([authority])
      .rpc();

    const pool = await find_pool(flashloan.publicKey, mint.publicKey);
    const [tokenAuthority, _nonce] = await find_token_authority(flashloan.publicKey);
    const lpTokenMint = await find_lp_token_mint(flashloan.publicKey, mint.publicKey)
    await create_token(lp_token1, lpTokenMint, provider.wallet.publicKey);

    await program.methods
      .deposit(new BN(1000 * web3.LAMPORTS_PER_SOL))
      .accounts({
        flashloan: flashloan.publicKey,
        pool,
        userToken: token1.publicKey,
        userLpToken: lp_token1.publicKey,
      })
      .preInstructions(
        [
          await spl_token.methods
            .approve(new BN(1000 * web3.LAMPORTS_PER_SOL))
            .accounts({
              source: token1.publicKey,
              delegate: tokenAuthority,
              authority: provider.wallet.publicKey
            }).instruction()
        ]
      )
      .rpc();
  })

  it("Should borrow and repay", async () => {
    const pool = await find_pool(flashloan.publicKey, mint.publicKey);
    const [tokenAuthority, _nonce] = await find_token_authority(flashloan.publicKey);
    const poolToken = await find_pool_token(flashloan.publicKey, mint.publicKey);

    let poolTokenAccount = await spl_token.account.token.fetch(poolToken);
    expect(poolTokenAccount.amount.toNumber()).to.be.equal(1000 * web3.LAMPORTS_PER_SOL);

    await spl_token.methods
      .mintTo(new BN(0.1 * web3.LAMPORTS_PER_SOL))
      .accounts(
        {
          mint: mint.publicKey,
          to: token2.publicKey,
          authority: provider.wallet.publicKey,
        })
      .rpc();

    await program.methods
      .borrow(new BN(100 * web3.LAMPORTS_PER_SOL))
      .accounts({
        flashloan: flashloan.publicKey,
        pool,
        userToken: token2.publicKey,
        instructions: web3.SYSVAR_INSTRUCTIONS_PUBKEY,
      })
      .preInstructions(
        [
          await spl_token.methods
            .approve(new BN(101 * web3.LAMPORTS_PER_SOL))
            .accounts({
              source: token2.publicKey,
              delegate: tokenAuthority,
              authority: provider.wallet.publicKey
            }).instruction()
        ]
      )
      .postInstructions(
        [
          await program.methods
            .repay(new BN(100.1 * web3.LAMPORTS_PER_SOL))
            .accounts({
              flashloan: flashloan.publicKey,
              pool,
              userToken: token2.publicKey,
              instructions: web3.SYSVAR_INSTRUCTIONS_PUBKEY,
            })
            .instruction()
        ]
      )
      .rpc();

    poolTokenAccount = await spl_token.account.token.fetch(poolToken);
    expect(poolTokenAccount.amount.toNumber()).to.be.equal(1000.1 * web3.LAMPORTS_PER_SOL);


  });
});
