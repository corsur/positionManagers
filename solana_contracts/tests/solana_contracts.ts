import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { SolanaContracts } from "../target/types/solana_contracts";

describe("solana_contracts", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());

  const program = anchor.workspace.SolanaContracts as Program<SolanaContracts>;

  it("Is initialized!", async () => {
    // Add your test here.
    const tx = await program.methods.initialize().rpc();
    console.log("Your transaction signature", tx);
  });
});
