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
  const VRF_PROGRAM_ID = new anchor.web3.PublicKey(
    "Vrf1RNUjXmQGjmQrQLvJHs9SNkvDJEsRVFPkfSQUwGz" // Replace with actual VRF program ID
  );
  const program = anchor.workspace.TokenLottery as Program<Raffle>;

  const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
    "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
  );

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
    const tx = await program.methods
      .commitWinner()
      .accounts({
        payer: wallet.publicKey,
        //@ts-ignore
        tokenLottery: anchor.web3.PublicKey.findProgramAddressSync(
          [Buffer.from("token_lottery")],
          program.programId
        )[0],

      })
      .rpc()


    console.log("Winner commit tx:", tx);
  });

  it("Wait for some seconds", async () => {
    await new Promise((resolve) => setTimeout(resolve, 15000));
  })


  it("Waits for VRF callback to choose winner", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    )[0];

    console.log("Waiting for VRF callback...");
    for (let i = 0; i < 30; i++) { // check for up to ~60 seconds
      await new Promise((r) => setTimeout(r, 2000));

      const lotteryConfig = await program.account.tokenLottery.fetch(tokenLotteryAddress);
      if (lotteryConfig.winnerChosen) {
        console.log("Winner chosen! Winner index:", lotteryConfig.winner.toString());
        return;
      }
    }

    throw new Error("Winner was not chosen after waiting.");
  });


  it("Claims winnings", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    )[0];

    const lotteryConfig = await program.account.tokenLottery.fetch(tokenLotteryAddress);
    console.log("Lottery config:", lotteryConfig);

    const winningMint = anchor.web3.PublicKey.findProgramAddressSync(
      [new anchor.BN(lotteryConfig.winner).toArrayLike(Buffer, "le", 8)],
      program.programId
    )[0];

    const winningTokenAddress = getAssociatedTokenAddressSync(winningMint, wallet.publicKey);

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

    console.log("Winning Token Address: ", winningTokenAddress)

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("Claimed winnings:", sig);
  });
});









