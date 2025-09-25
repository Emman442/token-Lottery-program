import * as anchor from "@coral-xyz/anchor";
import { asV0Tx, ON_DEMAND_DEVNET_PID, ON_DEMAND_MAINNET_PID, Queue, Randomness } from "@switchboard-xyz/on-demand";
import { Program } from "@coral-xyz/anchor";
import { TokenLottery } from "../target/types/token_lottery";
import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
import { getAssociatedTokenAddressSync } from "@solana/spl-token";

describe("token-lottery", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  const connection = provider.connection;
  const wallet = provider.wallet as anchor.Wallet;
  anchor.setProvider(provider);

  const program = anchor.workspace.TokenLottery as Program<TokenLottery>;
  let switchboardProgram;
  const rngKp = anchor.web3.Keypair.generate();
  let randomnessAccount: Randomness;

  const TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey('metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s');

  before("Loading switchboard program", async () => {
    const switchboardIDL = await anchor.Program.fetchIdl(
      ON_DEMAND_DEVNET_PID,
      { connection: new anchor.web3.Connection("https://api.devnet.solana.com") }
    );
    switchboardProgram = new anchor.Program(switchboardIDL, provider);
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

  it("Is initialized!", async () => {
    const slot = await connection.getSlot();
    console.log("Current slot", slot);

    const mint = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('collection_mint')],
      program.programId,
    )[0];

    const metadata = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('metadata'), TOKEN_METADATA_PROGRAM_ID.toBuffer(), mint.toBuffer()],
      TOKEN_METADATA_PROGRAM_ID,
    )[0];

    const masterEdition = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('metadata'), TOKEN_METADATA_PROGRAM_ID.toBuffer(), mint.toBuffer(), Buffer.from('edition')],
      TOKEN_METADATA_PROGRAM_ID,
    )[0];

    // FIX 1: Use current slot as start time and give more time for lottery to remain open
    const startTime = slot;
    const endTime = slot + 100000; // Give plenty of time for testing

    const initConfigIx = await program.methods.initializeConfig(
      new anchor.BN(startTime),
      new anchor.BN(endTime),
      new anchor.BN(10000),
    ).instruction();

    const initLotteryIx = await program.methods.initializeLottery()
      .accounts({
        //@ts-ignore
        masterEdition: masterEdition,
        metadata: metadata,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .instruction();

    const blockhashContext = await connection.getLatestBlockhash();

    const tx = new anchor.web3.Transaction({
      blockhash: blockhashContext.blockhash,
      lastValidBlockHeight: blockhashContext.lastValidBlockHeight,
      feePayer: wallet.payer.publicKey,
    }).add(initConfigIx)
      .add(initLotteryIx);

    const sig = await anchor.web3.sendAndConfirmTransaction(connection, tx, [wallet.payer]);
    console.log(sig);
  });

  it("Is buying tickets!", async () => {
    await buyTicket();
    await buyTicket();
    await buyTicket();
    await buyTicket();
    await buyTicket();
  });

  it("Is committing and revealing a winner", async () => {
    const queue = new anchor.web3.PublicKey("EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7"); //devnet

    const queueAccount = new Queue(switchboardProgram, queue);
    console.log("Queue account", queue.toString());
    try {
      await queueAccount.loadData();
    } catch (err) {
      console.log("Queue account not found");
      process.exit(1);
    }

    const [randomness, ix] = await Randomness.create(switchboardProgram, rngKp, queue);
    randomnessAccount = randomness; // Store for later use

    console.log("Created randomness account..");
    console.log("Randomness account", randomness.pubkey.toBase58());
    console.log("rkp account", rngKp.publicKey.toBase58());

    const createRandomnessTx = await asV0Tx({
      connection: connection,
      ixs: [ix],
      payer: wallet.publicKey,
      signers: [wallet.payer, rngKp],
      computeUnitPrice: 75_000,
      computeUnitLimitMultiple: 1.3,
    });

    const blockhashContext = await connection.getLatestBlockhashAndContext();

    const createRandomnessSignature = await connection.sendTransaction(createRandomnessTx);
    await connection.confirmTransaction({
      signature: createRandomnessSignature,
      blockhash: blockhashContext.value.blockhash,
      lastValidBlockHeight: blockhashContext.value.lastValidBlockHeight
    });
    console.log(
      "Transaction Signature for randomness account creation: ",
      createRandomnessSignature
    );

    const sbCommitIx = await randomness.commitIx(queue);

    const commitIx = await program.methods.commitAWinner()
      .accounts({
        //@ts-ignore
        randomnessAccount: randomness.pubkey,
        // FIX 2: Add the randomnessAccountData account if required by your program
        // You may need to check your program's IDL to see the exact account name
      })
      .instruction();

    const commitTx = await asV0Tx({
      connection: switchboardProgram.provider.connection,
      ixs: [sbCommitIx, commitIx],
      payer: wallet.publicKey,
      signers: [wallet.payer],
      computeUnitPrice: 75_000,
      computeUnitLimitMultiple: 1.3,
    });

    const commitSignature = await connection.sendTransaction(commitTx);
    await connection.confirmTransaction({
      signature: commitSignature,
      blockhash: blockhashContext.value.blockhash,
      lastValidBlockHeight: blockhashContext.value.lastValidBlockHeight
    });
    console.log(
      "Transaction Signature for commit: ",
      commitSignature
    );

    const sbRevealIx = await randomness.revealIx();
    const revealIx = await program.methods.chooseWinner()
      .accounts({
        //@ts-ignore
        randomnessAccount: randomness.pubkey,
        // Add randomnessAccountData here too if needed
      })
      .instruction();

    const revealTx = await asV0Tx({
      connection: switchboardProgram.provider.connection,
      ixs: [sbRevealIx, revealIx],
      payer: wallet.publicKey,
      signers: [wallet.payer],
      computeUnitPrice: 75_000,
      computeUnitLimitMultiple: 1.3,
    });

    const revealSignature = await connection.sendTransaction(revealTx);
    await connection.confirmTransaction({
      signature: revealSignature, // FIX: Use revealSignature instead of commitSignature
      blockhash: blockhashContext.value.blockhash,
      lastValidBlockHeight: blockhashContext.value.lastValidBlockHeight
    });
    console.log("  Transaction Signature revealTx", revealSignature);
  });

  it("Is claiming a prize", async () => {
    const tokenLotteryAddress = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from('token_lottery')],
      program.programId,
    )[0];
    const lotteryConfig = await program.account.tokenLottery.fetch(tokenLotteryAddress);
    console.log("Lottery winner", lotteryConfig.winner);
    console.log("Lottery config", lotteryConfig);

    const tokenAccounts = await connection.getParsedTokenAccountsByOwner(wallet.publicKey, { programId: TOKEN_PROGRAM_ID });
    tokenAccounts.value.forEach(async (account) => {
      console.log("Token account mint", account.account.data.parsed.info.mint);
      console.log("Token account address", account.pubkey.toBase58());
    });

    const winningMint = anchor.web3.PublicKey.findProgramAddressSync(
      [new anchor.BN(lotteryConfig.winner).toArrayLike(Buffer, 'le', 8)],
      program.programId,
    )[0];
    console.log("Winning mint", winningMint.toBase58());

    const winningTokenAddress = getAssociatedTokenAddressSync(
      winningMint,
      wallet.publicKey
    );
    console.log("Winning token address", winningTokenAddress.toBase58());

    const claimIx = await program.methods.claimWinnings()
      .accounts({
        tokenProgram: TOKEN_PROGRAM_ID,
        // FIX 3: You need to explicitly pass the ticket mint account
        // Check your program IDL to see what accounts are required
        // ticketMint: winningMint, // Add this if required by your program
        // ticketTokenAccount: winningTokenAddress, // Add this if required
      })
      .instruction();

    const blockhashContext = await connection.getLatestBlockhash();

    const claimTx = new anchor.web3.Transaction({
      blockhash: blockhashContext.blockhash,
      lastValidBlockHeight: blockhashContext.lastValidBlockHeight,
      feePayer: wallet.payer.publicKey,
    }).add(claimIx);

    const claimSig = await anchor.web3.sendAndConfirmTransaction(connection, claimTx, [wallet.payer]);
    console.log(claimSig);
  });
});
































// import * as anchor from "@coral-xyz/anchor";
// import { Program } from "@coral-xyz/anchor";
// import { TokenLottery } from "../target/types/token_lottery";
// import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
// import { ComputeBudgetProgram, Transaction } from "@solana/web3.js"
// import { ON_DEMAND_MAINNET_PID, ON_DEMAND_DEVNET_PID, AnchorUtils, Queue, Randomness, asV0Tx } from "@switchboard-xyz/on-demand"
// describe("token-lottery", () => {
//   // Configure the client to use the local cluster.
//   const provider = anchor.AnchorProvider.env();
//   anchor.setProvider(provider);
//   const wallet = provider.wallet as anchor.Wallet;
//   const program = anchor.workspace.tokenLottery as Program<TokenLottery>;

//   let switchboardProgram;
//   const rngKp = anchor.web3.Keypair.generate()

//   before("Load switchboard program", async () => {
//     const switchboardIDL = await anchor.Program.fetchIdl(
//       ON_DEMAND_DEVNET_PID,
//       { connection: new anchor.web3.Connection("https://api.devnet.solana.com") }
//     );
//     switchboardProgram = new anchor.Program(switchboardIDL, provider);
//   })

//   async function buyTicket() {
//     const buyTicketIx = await program.methods.buyTicket().accounts({ tokenProgram: TOKEN_PROGRAM_ID }).instruction()

//     const modifyComputeUnits = ComputeBudgetProgram.setComputeUnitLimit({
//       units: 400_000,
//     });

//     const addPriorityFee = ComputeBudgetProgram.setComputeUnitPrice({
//       microLamports: 1,
//     });

//     const blockchashWithContext = await provider.connection.getLatestBlockhash();

//     const txx = new anchor.web3.Transaction({
//       blockhash: blockchashWithContext.blockhash,
//       lastValidBlockHeight: blockchashWithContext.lastValidBlockHeight,
//       feePayer: provider.wallet.publicKey,
//     }).add(modifyComputeUnits).add(addPriorityFee).add(buyTicketIx)

//     const sig = await anchor.web3.sendAndConfirmTransaction(provider.connection, txx, [wallet.payer]);
//     // return sig

//     console.log("Your Transaction Signature: ", sig)
//   }

//   it("Should Init!", async () => {
//     // Add your test here.
//     // const initConfigTx = await program.methods.initializeConfig(new anchor.BN(0), new anchor.BN(1857774695), new anchor.BN(10000)).rpc();
//     //Alternatively 
//     const initConfigTx = await program.methods.initializeConfig(new anchor.BN(0), new anchor.BN(1857774695), new anchor.BN(10000)).instruction()
//     const blockhash = await provider.connection.getLatestBlockhash();
//     const tx = new anchor.web3.Transaction({
//       blockhash: blockhash.blockhash,
//       feePayer: wallet.publicKey,
//       lastValidBlockHeight: blockhash.lastValidBlockHeight
//     }).add(initConfigTx)

//     const signature = await anchor.web3.sendAndConfirmTransaction(provider.connection, tx, [wallet.payer])
//     console.log("Your transaction signature", signature);

//     const InitLotteryIx = await program.methods.initializeLottery().accounts({
//       tokenProgram: TOKEN_PROGRAM_ID
//     }).instruction()


//     const initLotteryTx = new anchor.web3.Transaction({
//       blockhash: blockhash.blockhash,
//       feePayer: wallet.publicKey,
//       lastValidBlockHeight: blockhash.lastValidBlockHeight
//     }).add(InitLotteryIx)

//     const initLotterySignature = await anchor.web3.sendAndConfirmTransaction(provider.connection, initLotteryTx, [wallet.payer]);
//     console.log("Your Init Lottery Signature: ", initLotterySignature)

//     await buyTicket()
//     await buyTicket()
//     await buyTicket()
//     await buyTicket()
//     await buyTicket()
//     await buyTicket()

//     const queue = new anchor.web3.PublicKey(
//       "EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7"
//     );

//     const queueAccount = new Queue(switchboardProgram, queue)

//     try {
//       await queueAccount.loadData();
//     } catch (error) {
//       console.log("Error", error);
//       process.exit(1)
//     }

//     const [randomness, createRandomnessIx] = await Randomness.create(switchboardProgram, rngKp, queue);
//     const createRandomnessTx = await asV0Tx({
//       connection: provider.connection,
//       ixs: [createRandomnessIx],
//       payer: wallet.publicKey,
//       signers: [wallet.payer, rngKp]
//     })

//     const createRandomnessSignature = await provider.connection.sendTransaction(createRandomnessTx)

//     console.log("Create Randomness Signature: ", createRandomnessSignature)


//     const commitIx = await program.methods.commitRandomness().accounts({
//       //@ts-ignore
//       randomness: randomness.pubkey
//     }).instruction()

//     const switchCommitIx = await randomness.commitIx(queue)
//     const commitComputeIx = anchor.web3.ComputeBudgetProgram.setComputeUnitLimit({
//       units: 100_000
//     })

//     const commitPriorityIx = ComputeBudgetProgram.setComputeUnitPrice({
//       microLamports: 1
//     })

//     const commitBlockhashWithContext = await provider.connection.getLatestBlockhash();

//     const commitTx = new Transaction({
//       feePayer: provider.wallet.publicKey,
//       blockhash: commitBlockhashWithContext.blockhash,
//       lastValidBlockHeight: commitBlockhashWithContext.lastValidBlockHeight
//     }).add(commitComputeIx).add(commitPriorityIx).add(switchCommitIx).add(commitIx)



//     const commitSignature = await anchor.web3.sendAndConfirmTransaction(provider.connection, commitTx, [wallet.payer], {skipPreflight: true})
//     console.log("Commit Signature: ", commitSignature  )

//   });



// });
