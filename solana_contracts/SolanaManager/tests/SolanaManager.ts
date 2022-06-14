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

  // it("Is initialized!", async () => {
  //   // Add your test here.
  //   const tx = await program.methods.initialize().rpc();
  //   console.log("Your transaction signature", tx);
  // });

  it('creates and gets a position', async () => {
    const [positionPDA, _] = await PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("position"),
          provider.wallet.publicKey.toBuffer()
        ],
        program.programId
      );

    await program.methods
      .getPositions()
      .accounts({
        user: provider.wallet.publicKey,
        position: positionPDA
      })
      .rpc();

    expect((await program.account.position.fetch(positionPDA)).name).to.equal("set");
  });
});
