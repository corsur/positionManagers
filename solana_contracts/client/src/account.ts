import { AccountLayout, Token, TOKEN_PROGRAM_ID } from "@solana/spl-token";
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  SYSVAR_RENT_PUBKEY,
  Transaction,
  TransactionInstruction,
  LAMPORTS_PER_SOL,
  Signer,
} from "@solana/web3.js";
import BN = require("bn.js");
import {
  PdnLayout,
  PDN_ACCOUNT_DATA_LAYOUT,
  getKeypair,
  getProgramId,
  getPublicKey,
  getDeposit,
  getTokenBalance,
  logError,
  writePublicKey,
} from "./utils";

const createMint = (
  connection: Connection,
  { publicKey, secretKey }: Signer
) => {
  return Token.createMint(
    connection,
    {
      publicKey,
      secretKey,
    },
    /*mintAuthority=*/publicKey,
    /*freezeAuthority=*/null,
    /*decimals=*/0,
    TOKEN_PROGRAM_ID
  );
};

const setupMint = async (
  name: string,
  connection: Connection,
  userPublicKey: PublicKey,
  clientKeypair: Signer
): Promise<[Token, PublicKey]> => {
  console.log(`Creating Mint ${name}...`);
  const mint = await createMint(connection, clientKeypair);
  writePublicKey(mint.publicKey, `mint_${name.toLowerCase()}`);

  console.log(`Creating User TokenAccount for ${name}...`);
  const usdcTokenAccount = await mint.createAccount(userPublicKey);
  writePublicKey(usdcTokenAccount, `${name.toLowerCase()}`);

  return [mint, usdcTokenAccount];
};

const setup = async () => {
  const userPublicKey = getPublicKey("user");
  const clientKeypair = getKeypair("client");

  const connection = new Connection("http://localhost:8899", "confirmed");
  console.log("Requesting SOL for User...");
  await connection.requestAirdrop(userPublicKey, LAMPORTS_PER_SOL * 10);
  console.log("Requesting SOL for Client...");
  await connection.requestAirdrop(
    clientKeypair.publicKey,
    LAMPORTS_PER_SOL * 10
  );

  const [mintX, usdcTokenAccount] = await setupMint(
    "usdc",
    connection,
    userPublicKey,
    clientKeypair,
  );
  console.log("Sending 50 usdc to user's usdc TokenAccount...");
  await mintX.mintTo(usdcTokenAccount, clientKeypair.publicKey, [], 50);

  const [mintY, borrowTokenAccount] = await setupMint(
    "borrow",
    connection,
    userPublicKey,
    clientKeypair
  );

  console.log("✨Setup complete✨\n");
  console.table([
    {
      "User Usdc Account": await getTokenBalance(
        usdcTokenAccount,
        connection
      ),
      "User Borrow Account": await getTokenBalance(
        borrowTokenAccount,
        connection
      ),
    },
  ]);
  console.log("");
};

const initializeAccounts = async () => {
  const pdnProgramId = getProgramId();
  const deposit = getDeposit();
  const usdcTokenAccountPubkey = getPublicKey("usdc");
  const borrowTokenAccountPubkey = getPublicKey("borrow");
  const usdcTokenMintPubkey = getPublicKey("mint_usdc");
  const keypair = getKeypair("user");

  const tempUsdcTokenAccountKeypair = new Keypair();
  const connection = new Connection("http://localhost:8899", "confirmed");

  // create temp token account of user
  // this account is for user's usdc token account
  const createTempTokenAccountIx = SystemProgram.createAccount({
    programId: TOKEN_PROGRAM_ID,
    space: AccountLayout.span,
    lamports: await connection.getMinimumBalanceForRentExemption(
      AccountLayout.span
    ),
    fromPubkey: keypair.publicKey,
    newAccountPubkey: tempUsdcTokenAccountKeypair.publicKey,
  });
  const initTempAccountIx = Token.createInitAccountInstruction(
    TOKEN_PROGRAM_ID,
    usdcTokenMintPubkey,
    tempUsdcTokenAccountKeypair.publicKey,
    keypair.publicKey
  );

  const transferUsdcTokensToTempAccIx = Token.createTransferInstruction(
    TOKEN_PROGRAM_ID,
    usdcTokenAccountPubkey,
    tempUsdcTokenAccountKeypair.publicKey,
    keypair.publicKey,
    [],
    deposit.usdcDepositAmount
  );

  // create temp account of pdn program
  // this account is for save user's 3 accounts: main account, usdc account, swap token account
  const pdnKeypair = new Keypair();
  const createPdnAccountIx = SystemProgram.createAccount({
    space: PDN_ACCOUNT_DATA_LAYOUT.span,
    lamports: await connection.getMinimumBalanceForRentExemption(
      PDN_ACCOUNT_DATA_LAYOUT.span
    ),
    fromPubkey: keypair.publicKey,
    newAccountPubkey: pdnKeypair.publicKey,
    programId: pdnProgramId,
  });
  const initPdnIx = new TransactionInstruction({
    programId: pdnProgramId,
    keys: [
      { pubkey: keypair.publicKey, isSigner: true, isWritable: false },
      {
        pubkey: tempUsdcTokenAccountKeypair.publicKey,
        isSigner: false,
        isWritable: true,
      },
      {
        pubkey: borrowTokenAccountPubkey,
        isSigner: false,
        isWritable: false,
      },
      { pubkey: pdnKeypair.publicKey, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
    ],
    data: Buffer.from(
      Uint8Array.of(0, ...new BN(deposit.usdcDepositAmount).toArray("le", 800))
    ),
  });

  // make a transaction, transfer usdc from main account to temp usdc account
  const tx = new Transaction().add(
    createTempTokenAccountIx,
    initTempAccountIx,
    transferUsdcTokensToTempAccIx,
    createPdnAccountIx,
    initPdnIx
  );
  console.log("Deposit USDC transaction...");
  await connection.sendTransaction(
    tx,
    [keypair, tempUsdcTokenAccountKeypair, pdnKeypair],
    { skipPreflight: false, preflightCommitment: "confirmed" }
  );

  // sleep to allow time to update
  await new Promise((resolve) => setTimeout(resolve, 1000));

  const pdnAccount = await connection.getAccountInfo(
    pdnKeypair.publicKey
  );

  if (pdnAccount === null || pdnAccount.data.length === 0) {
    logError("Pdn account has not been initialized properly");
    process.exit(1);
  }

  const encodedPdnState = pdnAccount.data;
  const decodedPdnState = PDN_ACCOUNT_DATA_LAYOUT.decode(
    encodedPdnState
  ) as PdnLayout;

  if (!decodedPdnState.isInitialized) {
    logError("Pdn state initialization flag has not been set");
    process.exit(1);
  } else if (
    !new PublicKey(decodedPdnState.initializerPubkey).equals(
      keypair.publicKey
    )
  ) {
    logError(
      "InitializerPubkey has not been set correctly / not been set to user's public key"
    );
    process.exit(1);
  } else if (
    !new PublicKey(
      decodedPdnState.initializerSwapTokenAccountPubkey
    ).equals(borrowTokenAccountPubkey)
  ) {
    logError(
      "initializerSwapTokenAccountPubkey has not been set correctly / not been set to user's swap token public key"
    );
    process.exit(1);
  } else if (
    !new PublicKey(decodedPdnState.initializerUsdcTokenAccountPubkey).equals(
      tempUsdcTokenAccountKeypair.publicKey
    )
  ) {
    logError(
      "initializerUsdcTokenAccountPubkey has not been set correctly / not been set to temp USDC token account public key"
    );
    process.exit(1);
  }
  console.log(
    `✨PDN successfully initialized. User deposited ${deposit.usdcDepositAmount} USDC✨\n`
  );
  writePublicKey(pdnKeypair.publicKey, "pdn");
  console.table([
    {
      "User Token Account USDC": await getTokenBalance(
        usdcTokenAccountPubkey,
        connection
      ),
      "User Token Account Swap token": await getTokenBalance(
        borrowTokenAccountPubkey,
        connection
      ),
      "Temporary Token Account USDC": await getTokenBalance(
        tempUsdcTokenAccountKeypair.publicKey,
        connection
      ),
    },
  ]);

  console.log("");
};

setup();
initializeAccounts();
