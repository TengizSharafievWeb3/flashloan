import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { Flashloan } from "../target/types/flashloan";

describe("flashloan", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.Provider.env());

  const program = anchor.workspace.Flashloan as Program<Flashloan>;

  it("Is initialized!", async () => {
    // Add your test here.
    const tx = await program.rpc.initialize({});
    console.log("Your transaction signature", tx);
  });
});
