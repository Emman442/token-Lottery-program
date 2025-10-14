import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Raffle } from "../target/types/raffle";
import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
import {
  createMint,
  getAssociatedTokenAddressSync,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";

const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

describe("token-lottery full cycle", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const connection = provider.connection;
  const wallet = provider.wallet as anchor.Wallet;
  const program = anchor.workspace.Raffle as Program<Raffle>;

  let tokenMint: anchor.web3.PublicKey;
  let userTokenAccount: anchor.web3.PublicKey;
  let vaultTokenAccount: anchor.web3.PublicKey;
  let tokenLotteryPda: anchor.web3.PublicKey;
  let tokenLotteryBump: number;

  before(async () => {
    console.log("🔧 Setting up test mint and accounts...");

    tokenMint = await createMint(
      connection,
      wallet.payer,
      wallet.publicKey,
      null,
      6 // decimals
    );

    const userATA = await getOrCreateAssociatedTokenAccount(
      connection,
      wallet.payer,
      tokenMint,
      wallet.publicKey
    );
    userTokenAccount = userATA.address;

    await mintTo(
      connection,
      wallet.payer,
      tokenMint,
      userTokenAccount,
      wallet.payer,
      1_000_000_000 // 1000 tokens
    );

    [tokenLotteryPda, tokenLotteryBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    );

    vaultTokenAccount = getAssociatedTokenAddressSync(tokenMint, tokenLotteryPda, true);

    // ✅ Initialize Config ONCE (only in before hook)
    const startTime = Math.floor(Date.now() / 1000) - 10;  // Started 10 seconds ago
    const endTime = Math.floor(Date.now() / 1000) + 60;   // Ends 60 seconds from now (plenty of time)

    const computeIx = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 300_000,
    });

    const priorityIx = anchor.web3.ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: 1,
    });

    const initConfigIx = await program.methods
      .initializeConfig(new anchor.BN(startTime), new anchor.BN(endTime), new anchor.BN(10000))
      .instruction();

    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
    const tx = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(initConfigIx)
      .add(computeIx)
      .add(priorityIx);

    await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("✅ Initialized config (one-time setup)");
  });

  async function runLotteryRound(roundNumber: number) {
    console.log(`\n🎯 Running Lottery Round ${roundNumber}...\n`);

    const computeIx = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 300_000,
    });

    const priorityIx = anchor.web3.ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: 1,
    });

    // If not the first round, restart the lottery
    if (roundNumber > 0) {
      const startTime = Math.floor(Date.now() / 1000) - 10;  // Started 10 seconds ago
      const endTime = Math.floor(Date.now() / 1000) + 60;   // Ends 60 seconds from now

      const restartIx = await program.methods
        .restartLottery(new anchor.BN(startTime), new anchor.BN(endTime), new anchor.BN(10000))
        .accounts({
          //@ts-ignore
          tokenLottery: tokenLotteryPda,
          authority: wallet.publicKey,
        })
        .instruction();

      const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
      const txRestart = new anchor.web3.Transaction({
        blockhash,
        lastValidBlockHeight,
        feePayer: wallet.publicKey,
      }).add(restartIx)
        .add(computeIx)
        .add(priorityIx);

      await anchor.web3.sendAndConfirmTransaction(connection, txRestart, [wallet.payer]);
      console.log("✅ Restarted lottery for round", roundNumber);
    }

    // Fetch current round_id from the account
    const tokenLottery = await program.account.tokenLottery.fetch(tokenLotteryPda);
    const roundId = tokenLottery.roundId;

    const [collectionMint] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("collection_mint"),
        new anchor.BN(roundId).toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );

    const collectionTokenAccount = getAssociatedTokenAddressSync(
      collectionMint,
      collectionMint,
      true
    );

    const [metadata] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), collectionMint.toBuffer()],
      TOKEN_METADATA_PROGRAM_ID
    );

    const [masterEdition] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), collectionMint.toBuffer(), Buffer.from("edition")],
      TOKEN_METADATA_PROGRAM_ID
    );

    // ✅ Initialize Lottery (creates new collection for this round)
    const initLotteryIx = await program.methods
      .initializeLottery()
      .accounts({
        ///@ts-ignore
        tokenLottery: tokenLotteryPda,
        masterEdition,
        ///@ts-ignore
        systemProgram: anchor.web3.SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        collectionMint,
        collectionTokenAccount,
        metadata,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .instruction();

    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
    const tx1 = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(initLotteryIx)
      .add(computeIx)
      .add(priorityIx);

    await anchor.web3.sendAndConfirmTransaction(connection, tx1, [wallet.payer]);
    console.log("✅ Initialized lottery for round", roundId);

    // ✅ Buy Ticket
    const buyIx = await program.methods
      .buyTicket()
      .accounts({
        payer: wallet.publicKey,
        payerTokenAccount: userTokenAccount,
        //@ts-ignore
        raffleVaultAccount: vaultTokenAccount,
        tokenMint,
        tokenLottery: tokenLotteryPda,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .instruction();

    const tx2 = new anchor.web3.Transaction().add(buyIx).add(computeIx).add(priorityIx);
    await anchor.web3.sendAndConfirmTransaction(connection, tx2, [wallet.payer]);
    console.log("🎟️ Ticket purchased");

    // ✅ Wait for lottery to end
    console.log("⏳ Waiting for lottery to end...");
    await new Promise((resolve) => setTimeout(resolve, 65000)); // Wait 65 seconds to ensure it's past end_time

    // ✅ Commit Winner
    const tx3 = await program.methods
      .commitWinner(0)
      .accounts({ payer: wallet.publicKey })
      .rpc();
    console.log("🎲 Winner committed:", tx3);

    // ✅ Claim Prize
    const tokenLotteryUpdated = await program.account.tokenLottery.fetch(tokenLotteryPda);
    const roundIdBuffer = new anchor.BN(tokenLotteryUpdated.roundId).toArrayLike(Buffer, "le", 8);
    const winnerBuffer = new anchor.BN(tokenLotteryUpdated.winner).toArrayLike(Buffer, "le", 8);

    const [ticketMint] = anchor.web3.PublicKey.findProgramAddressSync(
      [roundIdBuffer, winnerBuffer],
      program.programId
    );

    const [ticketMetadata] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), ticketMint.toBuffer()],
      TOKEN_METADATA_PROGRAM_ID
    );

    const destination = getAssociatedTokenAddressSync(ticketMint, wallet.publicKey);

    const claimIx = await program.methods
      .claimWinnings()
      .accounts({
        payer: wallet.publicKey,
        winnerTokenAccount: userTokenAccount,
        //@ts-ignore
        metadata: ticketMetadata,
        destination,
        ticketMint,
        tokenLottery: tokenLotteryPda,
        rewardMint: tokenMint,
        rewardVault: vaultTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .instruction();

    const tx4 = new anchor.web3.Transaction().add(claimIx);
    await anchor.web3.sendAndConfirmTransaction(connection, tx4, [wallet.payer]);
    console.log("🏆 Prize claimed successfully!");
  }

  it("Runs multiple full lottery rounds", async () => {
    await runLotteryRound(0);
    await runLotteryRound(1);
    await runLotteryRound(2); // You can run as many rounds as you want!
  });
});