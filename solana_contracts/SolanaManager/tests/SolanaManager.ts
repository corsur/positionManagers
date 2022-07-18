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

  function getSingletonPDA(singletonSeed) {
    return PublicKey
      .findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode(singletonSeed)
        ],
        program.programId
      );
  }

  it('Initializes governance', async () => {

    const [adminInfoPDA, adminBump] = await getSingletonPDA("admininfo");

    await program.methods
      .initializeAdmin()
      .accounts({
        adminInfo: adminInfoPDA
      })
      .rpc();

    let admin_info = await program.account.adminInfo.fetch(adminInfoPDA);

    console.log("Admin from PDA: " + admin_info.admin.toString());
    console.log("Admin expected: " + wallet.publicKey.toBase58())
    expect(admin_info.admin.toString()).to.equal(wallet.publicKey.toBase58());

    const [feeSinkPDA, feeSinkBump] = await getSingletonPDA("feesink");

    await program.methods
      .initializeFeeSink(wallet.publicKey)
      .accounts({
        feeSink: feeSinkPDA,
        adminInfo: adminInfoPDA
      })
      .rpc();

    let fee_sink = await program.account.feeSink.fetch(feeSinkPDA);

    console.log("Fee sink from PDA: " + fee_sink.feeSink.toString());
    console.log("Fee sink expected: " + wallet.publicKey.toBase58())
    expect(fee_sink.feeSink.toString()).to.equal(wallet.publicKey.toBase58());
    
  });

  it('Updates fee sink', async () => {

    const [adminInfoPDA, adminBump] = await getSingletonPDA("admininfo");

    const [feeSinkPDA, feeSinkBump] = await getSingletonPDA("feesink");

    let keypair1 = Keypair.generate();

    await program.methods
      .updateFeeSink(keypair1.publicKey)
      .accounts({
        feeSink: feeSinkPDA,
        adminInfo: adminInfoPDA
      })
      .rpc();

    let fee_sink = await program.account.feeSink.fetch(feeSinkPDA);

    console.log("Fee sink from PDA: " + fee_sink.feeSink.toString());
    console.log("Fee sink expected: " + keypair1.publicKey.toBase58())
    expect(fee_sink.feeSink.toString()).to.equal(keypair1.publicKey.toBase58());
    
  });
  

  it('Creates an aperture manager', async () => {

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

    let keypair1 = Keypair.generate();

    const [adminInfoPDA, adminBump] = await getSingletonPDA("admininfo");

    await program.methods
      .initializeManager(sequence, keypair1.publicKey)
      .accounts({
        manager: managerPDA,
        adminInfo: adminInfoPDA
      })
      .rpc();
    
    expect((await program.account.apertureManager.fetch(managerPDA)).bump).to.equal(canonicalBump);
    console.log("Manager address in PDA: " + await (await program.account.apertureManager.fetch(managerPDA)).managerAddress);
    console.log("Manager address expected: " + keypair1.publicKey);
    expect((await program.account.apertureManager.fetch(managerPDA)).managerAddress.toBase58())
      .to.equal(keypair1.publicKey.toBase58());

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
