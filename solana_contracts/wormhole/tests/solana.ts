import { ApertureWormhole } from "../target/types/aperture_wormhole";
import * as anchor from '@project-serum/anchor';
import { Program, Wallet } from '@project-serum/anchor';
import { PublicKey, Keypair } from '@solana/web3.js';
import { expect } from 'chai';

describe("solana", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  const wallet = provider.wallet as Wallet;
  anchor.setProvider(provider);
  const program = anchor.workspace.ApertureWormhole as Program<ApertureWormhole>;

  let bridgeAddress = new PublicKey("3u8hJUVTA4jH1wYAyUur7FFZVQ8H635K3tSHHF4ssjQ5");


  function getSingletonPDA(singletonSeed, program) {
    return PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode(singletonSeed)
        ],
        program
      );
  }

  it("Sending message to wormhole", async () => {

    const [wormholeConfigPDA, wormholeConfigBump] = await getSingletonPDA("Bridge", bridgeAddress);
    const [wormholeFeeCollectorPDA, wormholeFeeCollectorBump] = await getSingletonPDA("fee_collector", bridgeAddress);
    const [wormholeDerivedEmitterPDA, wormholeDerivedEmitterBump] = await getSingletonPDA("emitter", program.programId);

    const [wormholeSequencePDA, wormholeSequenceBump] = 
    await PublicKey
    .findProgramAddress(
      [
        anchor.utils.bytes.utf8.encode("Sequence"),
        wormholeDerivedEmitterPDA.toBuffer()
      ],
      bridgeAddress
    )

    const [configPDA, configBump] = await getSingletonPDA("config", program.programId);

    
    const initializeTx = await program.methods
      .initialize()
      .accounts({config: configPDA})
      .rpc();

    console.log("Sequence: ");
    console.log(wormholeSequencePDA.toString());

    const tx = await program.methods
      .publishExecuteStrategyInstruction("HelloWorld")
      .accounts({
        coreBridge: bridgeAddress,
        wormholeConfig: wormholeConfigPDA,
        wormholeFeeCollector: wormholeFeeCollectorPDA,
        wormholeDerivedEmitter: wormholeDerivedEmitterPDA,
        wormholeSequence: wormholeSequencePDA,
        clock: anchor.web3.SYSVAR_CLOCK_PUBKEY,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        config: configPDA
      })
      .rpc();

    console.log("Your transaction signature", tx);

  });
});
