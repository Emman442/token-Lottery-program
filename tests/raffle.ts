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


describe("token-lottery", () => {
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
  let raffleVaultAccount: anchor.web3.PublicKey;
  let vaultBump: number;

  // ✅ Step 1. Create token mint & initial user ATA
  before(async () => {
    console.log("🔧 Setting up test mint and accounts...");

    // Create a new token mint
    tokenMint = await createMint(
      connection,
      wallet.payer,
      wallet.publicKey,
      null,
      6 // decimals
    );

    // Derive user's ATA and mint tokens to them
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
      1_000_000_000 // 1000 tokens with 6 decimals
    );

    // Derive token lottery PDA and vault ATA
    [tokenLotteryPda, tokenLotteryBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("token_lottery")],
      program.programId
    );



    vaultTokenAccount = getAssociatedTokenAddressSync(tokenMint, tokenLotteryPda, true);
  });

  // ✅ Step 2. Initialize the raffle config and vault
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


  // ✅ Step 3. Buy ticket (transfer tokens)
  it("Buys a ticket", async () => {

    const ix = await program.methods
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

    const computeIx = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({
      units: 300000
    });

    const priorityIx = anchor.web3.ComputeBudgetProgram.setComputeUnitPrice({
      microLamports: 1
    });


    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
    const tx = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(ix)
      .add(computeIx)
      .add(priorityIx);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("🎟️ Ticket purchase tx:", sig);
  });

  // ✅ Step 4. Commit winner (simplified)
  it("Commits to reveal a winner", async () => {
    const tx = await program.methods
      .commitWinner(0)
      .accounts({
        payer: wallet.publicKey,
      })
      .rpc();

    console.log("🎲 Winner commit tx:", tx);
  });

  // ✅ Step 5. Claim winnings (transfer back)
  it("Claims winnings", async () => {
    const tokenLottery = await program.account.tokenLottery.fetch(tokenLotteryPda);
    const [ticketMint] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from(new anchor.BN(tokenLottery.winner).toArrayLike(Buffer, "le", 8))], // winner = 0
      program.programId
    );


    const [ticketMetadata] = anchor.web3.PublicKey.findProgramAddressSync(
      [
        Buffer.from("metadata"),
        TOKEN_METADATA_PROGRAM_ID.toBuffer(),
        ticketMint.toBuffer(),
      ],
      TOKEN_METADATA_PROGRAM_ID
    );

    const destination = getAssociatedTokenAddressSync(ticketMint, wallet.publicKey);

    const ix = await program.methods
      .claimWinnings()
      .accounts({
        payer: wallet.publicKey,
        winnerTokenAccount: userTokenAccount,
        //@ts-ignore
        metadata: ticketMetadata,
        destination,
        ticketMint: ticketMint,
        tokenLottery: tokenLotteryPda,
        rewardMint: tokenMint,
        rewardVault: vaultTokenAccount,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .instruction();

    const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
    const tx = new anchor.web3.Transaction({
      blockhash,
      lastValidBlockHeight,
      feePayer: wallet.publicKey,
    }).add(ix);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log("🏆 Claimed winnings tx:", sig);
  });



  it("Restarts a lottery", async () => {
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



    const sig = await program.methods.restartLottery(new anchor.BN(startTime), new anchor.BN(endTime), new anchor.BN(10000))
      .accounts({
        ///@ts-ignore
        masterEdition,
        metadata,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log("Restarted lottery:", sig);
  });



});
