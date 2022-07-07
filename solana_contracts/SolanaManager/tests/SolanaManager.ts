import * as anchor from '@project-serum/anchor';
import { Program, Wallet } from '@project-serum/anchor';
import { PublicKey, Keypair } from '@solana/web3.js';
import { SolanaManager } from "../target/types/solana_manager";
import { expect } from 'chai';

describe("SolanaManager", () => {
  // Configure the client to use the local cluster.

  const provider = anchor.AnchorProvider.env();
  const wallet = provider.wallet as Wallet;
  anchor.setProvider(provider);
  const program = anchor.workspace.SolanaManager as Program<SolanaManager>;

  it('Creates and updates an aperture manager', async () => {
    let sequence = 3;
    const buffer = new anchor.BN(sequence).toArrayLike(Buffer);

    const [managerPDA, canonicalBump] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("manager"),
          wallet.publicKey.toBuffer(),
          buffer
        ],
        program.programId
      );

    console.log(canonicalBump);
    console.log(wallet.publicKey);

    let keypair1 = Keypair.generate();

    await program.methods
      .updateManager(sequence, keypair1.publicKey)
      .accounts({
        admin: wallet.publicKey,
        manager: managerPDA
      })
      .rpc();

    console.log((await program.account.apertureManager.fetch(managerPDA)).bump);
    
    expect((await program.account.apertureManager.fetch(managerPDA)).bump).to.equal(canonicalBump);

  });

  it('Creates and gets a position', async () => {
    const byte = new anchor.BN(0).toArrayLike(Buffer);

    const [positionPDA, originalBump] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("position"),
          provider.wallet.publicKey.toBuffer(),
          byte
        ],
        program.programId
      );

    console.log(originalBump);

    await program.methods
      .createPosition(0)
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA
      })
      .rpc();
  
    console.log((await program.account.position.fetch(positionPDA)).bump);
    
    expect((await program.account.position.fetch(positionPDA)).bump).to.equal(originalBump);

  });

  it('Creates and gets different positions', async () => {
    const byte1 = new anchor.BN(1).toArrayLike(Buffer);

    const [positionPDA1, originalBump] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("position"),
          provider.wallet.publicKey.toBuffer(),
          byte1
        ],
        program.programId
      );

    await program.methods
      .createPosition(1)
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA1
      })
      .rpc();

    console.log((await program.account.position.fetch(positionPDA1)).bump);
      
    const byte2 = new anchor.BN(2).toArrayLike(Buffer);

    const [positionPDA2, secondBump] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("position"),
          provider.wallet.publicKey.toBuffer(),
          byte2
        ],
        program.programId
      );

    await program.methods
      .createPosition(2)
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA2
      })
      .rpc();

      console.log((await program.account.position.fetch(positionPDA2)).bump);
      
    expect(originalBump != secondBump);

  });
});
