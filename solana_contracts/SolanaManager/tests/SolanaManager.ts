import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { SolanaManager } from "../target/types/solana_manager";

describe("SolanaManager", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.SolanaManager as Program<SolanaManager>;

  it("Is initialized!", async () => {
    // Add your test here.
    const tx = await program.methods.initialize().rpc();
    console.log("Your transaction signature", tx);
  });
});
