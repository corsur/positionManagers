import * as anchor from '@project-serum/anchor';
import { Program } from '@project-serum/anchor';
import { PublicKey, PublicKeyInitData } from '@solana/web3.js';
import { SolanaManager } from "../target/types/solana_manager";
import { expect } from 'chai';
import { FileSystemCredentials } from 'aws-sdk';

describe("SolanaManager", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.SolanaManager as Program<SolanaManager>;
  const provider = program.provider as anchor.AnchorProvider;

  it('Creates and gets a position', async () => {
    const [positionPDA, bump] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("position"),
          provider.wallet.publicKey.toBuffer()
        ],
        program.programId
      );

    console.log(typeof(bump));
    console.log(bump);

    await program.methods
      .createPosition()
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA
      })
      .rpc();

    const compareKey = await program.methods
      .getPositionPdas(provider.wallet.publicKey)
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA
      })
      .rpc();

      console.log(typeof(compareKey))
      console.log(compareKey);

    //expect((await program.account.position.fetch(positionPDA)) == 
    //  (await program.account.position.fetch(gotPositionPDA)))
  });
});
