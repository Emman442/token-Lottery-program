import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Raffle } from "../target/types/raffle";
import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";

describe("token-lottery", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const connection = provider.connection;
  const wallet = provider.wallet as anchor.Wallet;

  const program = anchor.workspace.Raffle as Program<Raffle>;

  const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  );

  // Check network and wallet balance
  before(async () => {
    const genesisHash = await connection.getGenesisHash();
    console.log("Connected to network - Genesis Hash:", genesisHash);
    console.log("RPC endpoint:", connection.rpcEndpoint);
    console.log("Program ID:", program.programId.toString());

    const balance = await connection.getBalance(wallet.publicKey);
    console.log("Wallet balance:", balance / anchor.web3.LAMPORTS_PER_SOL, "SOL");
    console.log("Wallet address:", wallet.publicKey.toString());

    if (balance < 0.1 * anchor.web3.LAMPORTS_PER_SOL) {
      console.warn("⚠️  Low balance! Get devnet SOL from https://faucet.solana.com");
    }
  });

  async function buyTicket() {
    const buyTicketIx = await program.methods.buyTicket()
      .accounts({
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .instruction();

    const blockhashContext = await connection.getLatestBlockhash();

    const computeIx = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 300000
    });

    const priorityIx = anchor.web3.ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: 1
    });

    const tx = new anchor.web3.Transaction({
      blockhash: blockhashContext.blockhash,
      lastValidBlockHeight: blockhashContext.lastValidBlockHeight,
      feePayer: wallet.payer.publicKey,
    }).add(buyTicketIx)
      .add(computeIx)
      .add(priorityIx);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("buy ticket ", sig);
  }

  it("Initializes config and lottery", async () => {
    const slot = await connection.getSlot();
    const startTime = slot;
    const endTime = slot + 10000;

    const mint = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("collection_mint")],
      program.programId
    )[0];

    const metadata = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), mint.toBuffer()],
      TOKEN_METADATA_PROGRAM_ID
    )[0];

    const masterEdition = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("metadata"), TOKEN_METADATA_PROGRAM_ID.toBuffer(), mint.toBuffer(), Buffer.from("edition")],
      TOKEN_METADATA_PROGRAM_ID
    )[0];

    const initConfigIx = await program.methods
      .initializeConfig(new anchor.BN(startTime), new anchor.BN(endTime), new anchor.BN(10000))
      .instruction();

    const initLotteryIx = await program.methods
      .initializeLottery()
      .accounts({
        ///@ts-ignore
        masterEdition,
        metadata,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .instruction();

    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
    const tx = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(initConfigIx).add(initLotteryIx);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("Initialized config & lottery:", sig);
  });

  it("Buys tickets", async () => {
    for (let i = 0; i < 5; i++) await buyTicket();
  });

  it("Commits to reveal a winner (MagicBlock VRF)", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    )[0];

    const tx = await program.methods
      .commitWinner(0)
      .accounts({
        payer: wallet.publicKey,
        //@ts-ignore
        tokenLottery: tokenLotteryAddress,
      })
      .rpc();

    console.log("Winner commit tx:", tx);

    // Wait for confirmation
    await connection.confirmTransaction(tx, "confirmed");

    // Fetch and display transaction logs
    const txDetails = await connection.getTransaction(tx, {
      commitment: "confirmed",
      maxSupportedTransactionVersion: 0
    });

    if (txDetails?.meta?.logMessages) {
      console.log("\n--- Transaction Logs ---");
      txDetails.meta.logMessages.forEach(log => console.log(log));
      console.log("--- End Logs ---\n");

      // Check if discriminator was logged
      const discriminatorLog = txDetails.meta.logMessages.find(log =>
        log.includes("Callback discriminator")
      );
      if (discriminatorLog) {
        console.log("✅ Discriminator found:", discriminatorLog);
      }
    }
  });

  it("Waits for VRF callback to choose winner", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    )[0];

    console.log("\n🔍 Waiting for VRF callback...");
    console.log("Token Lottery Address:", tokenLotteryAddress.toString());
    console.log("Program ID:", program.programId.toString());
    console.log("Checking every 3 seconds for up to 3 minutes...\n");

    let lastTxCount = 0;

    for (let i = 0; i < 60; i++) {
      await new Promise((r) => setTimeout(r, 3000));

      // Fetch lottery state
      const lotteryConfig = await program.account.tokenLottery.fetch(tokenLotteryAddress);

      const statusSymbol = lotteryConfig.winnerChosen ? "✅" : "⏳";
      console.log(
        `${statusSymbol} Check ${i + 1}/60 - Winner chosen: ${lotteryConfig.winnerChosen}, ` +
        `Winner: ${lotteryConfig.winner.toString()}, Total tickets: ${lotteryConfig.totalTickets.toString()}`
      );

      if (lotteryConfig.winnerChosen) {
        console.log("\n🎉 Winner chosen! Winner index:", lotteryConfig.winner.toString());
        return;
      }

      // Every 5 checks, look for recent transactions
      if (i % 5 === 0) {
        try {
          const signatures = await connection.getSignaturesForAddress(
            tokenLotteryAddress,
            { limit: 10 }
          );

          if (signatures.length !== lastTxCount) {
            console.log(`\n📝 Found ${signatures.length} transactions (was ${lastTxCount})`);
            lastTxCount = signatures.length;

            // Check the most recent transactions for callback
            for (const sig of signatures.slice(0, 3)) {
              const tx = await connection.getTransaction(sig.signature, {
                maxSupportedTransactionVersion: 0,
                commitment: "confirmed"
              });

              if (tx?.meta?.logMessages) {
                const hasCallbackLog = tx.meta.logMessages.some(log =>
                  log.includes("Callback invoked") ||
                  log.includes("🎲") ||
                  log.includes("CallbackChooseWinner")
                );

                if (hasCallbackLog) {
                  console.log("\n🎯 Found callback transaction:", sig.signature);
                  console.log("--- Callback Logs ---");
                  tx.meta.logMessages
                    .filter(log => !log.includes("consumed"))
                    .forEach(log => console.log(log));
                  console.log("--- End Callback Logs ---\n");
                }
              }
            }
          }
        } catch (err) {
          console.log("Error checking transactions:", err.message);
        }
      }
    }

    // If we get here, callback never happened
    console.log("\n❌ Winner was not chosen after waiting 3 minutes.");
    console.log("\nDebugging info:");
    console.log("- Make sure you're on devnet (not localnet)");
    console.log("- Check Anchor.toml has: cluster = \"devnet\"");
    console.log("- VRF oracles only run on devnet/mainnet");
    console.log("- Your program might need to be redeployed to devnet");

    throw new Error("Winner was not chosen after waiting.");
  });

  it("Claims winnings", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    )[0];

    const lotteryConfig = await program.account.tokenLottery.fetch(tokenLotteryAddress);
    console.log("\nLottery config:", {
      winner: lotteryConfig.winner.toString(),
      winnerChosen: lotteryConfig.winnerChosen,
      totalTickets: lotteryConfig.totalTickets.toString(),
      potAmount: lotteryConfig.potAmount.toString(),
    });

    const winningMint = anchor.web3.PublicKey.findProgramAddressSync(
      [new anchor.BN(lotteryConfig.winner).toArrayLike(Buffer, "le", 8)],
      program.programId
    )[0];

    const winningTokenAddress = getAssociatedTokenAddressSync(winningMint, wallet.publicKey);

    console.log("Winning Mint:", winningMint.toString());
    console.log("Winning Token Address:", winningTokenAddress.toString());

    const ix = await program.methods.claimWinnings()
      .accounts({
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .instruction();

    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();

    const tx = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(ix);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("✅ Claimed winnings:", sig);
  });
});
