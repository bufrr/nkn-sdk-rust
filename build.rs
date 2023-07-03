use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "protos/transaction.proto",
            "protos/payloads.proto",
            "protos/node.proto",
            "protos/sigchain.proto",
            "protos/clientmessage.proto",
            "protos/nodemessage.proto",
            "protos/block.proto",
            "protos/packet.proto",
        ],
        &["protos/"],
    )?;
    Ok(())
}
