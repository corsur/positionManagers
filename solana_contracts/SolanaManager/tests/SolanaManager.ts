import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { PublicKey } from '@solana/web3.js';
import { SolanaManager } from "../target/types/solana_manager";
import { expect } from 'chai';

describe("SolanaManager", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.SolanaManager as Program<SolanaManager>;
  const provider = program.provider as anchor.AnchorProvider;

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
