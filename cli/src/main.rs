use alloy::{
    hex,
    network::TransactionBuilder,
    primitives::{address, Address, Bytes, FixedBytes, B256, U256},
    providers::Provider,
    rpc::types::{Filter, Log},
    signers::local::PrivateKeySigner,
    sol_types::SolEvent,
};
use alloy_zksync::{
    network::transaction_request::TransactionRequest, provider::zksync_provider,
    wallet::ZksyncWallet,
};
use k256::ecdsa::SigningKey;

use alloy::sol;

use clap::Parser;
use std::{collections::HashMap, marker::PhantomData, str::FromStr};

sol! {
    #[sol(rpc)]
    contract InteropCenter {
        event InteropMessageSent(
            bytes32 indexed msgHash,
            address indexed sender,
            bytes payload
        );
    }
}

struct InteropMessageParsed {
    pub interop_center_sender: Address,
    // The unique global identifier of this message.
    pub msg_hash: FixedBytes<32>,
    // The address that sent this message on the source chain.
    pub sender: Address,

    // 'data' field from the Log (it contains the InteropMessage).
    pub data: Bytes,
}

impl InteropMessageParsed {
    pub fn from_log(log: &Log) -> Self {
        InteropMessageParsed {
            interop_center_sender: log.address(),
            msg_hash: log.topics()[1],
            sender: Address::from_slice(&log.topics()[2].0[12..]),
            data: log.data().data.clone(),
        }
    }

    pub fn is_type_b(&self) -> bool {
        return self.interop_center_sender == self.sender;
    }

    // Checks if the interop message is of type b.
}

struct InteropChain<P> {
    pub provider: P,
    pub interop_address: Address,
    pub rpc: String,
}

/*
impl<P, T, N> InteropChain<P, T, N>
where
    P: Provider<T, N>,
    T: Transport + Clone,
    N: Network,
{
    pub async fn get_recent_messages(&self, recent_blocks: u64) {
        let latest_block = self.provider.get_block_number().await.unwrap();
    }
}*/

#[derive(Parser, Debug)]
#[command(name = "Ethereum Interop CLI")]
#[command(version = "1.0")]
#[command(about = "Handles RPC URLs and interop Ethereum addresses")]
struct Cli {
    /// List of RPC URL and interop address pairs (e.g. -r URL ADDRESS)
    #[arg(short, long, num_args = 2, value_names = ["URL", "ADDRESS"])]
    rpc: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut rpc_addresses = Vec::new();

    // Process URL-address pairs
    for chunk in cli.rpc.chunks(2) {
        if chunk.len() == 2 {
            let url = &chunk[0];
            match Address::from_str(&chunk[1]) {
                Ok(address) => rpc_addresses.push((url.clone(), address)),
                Err(_) => eprintln!("Invalid Ethereum address: {}", chunk[1]),
            }
        } else {
            eprintln!("Each RPC URL must be paired with an Ethereum address.");
        }
    }

    let mut providers_map = HashMap::new();

    for (rpc, interop_address) in rpc_addresses {
        let provider = zksync_provider()
            .with_recommended_fillers()
            .on_http(rpc.clone().parse().unwrap());

        let chain_id = provider.get_chain_id().await?;
        let prev = providers_map.insert(
            chain_id,
            InteropChain {
                provider,
                interop_address,
                rpc: rpc.clone(),
            },
        );
        if let Some(prev) = prev {
            panic!(
                "Two interops with the same chain id {} -- {} and {} ",
                chain_id, rpc, prev.rpc
            );
        }
    }

    for (_, entry) in providers_map {
        let latest_block = entry.provider.get_block_number().await.unwrap();

        // Look at last 1k blocks.
        let filter = Filter::new()
            .from_block(latest_block.saturating_sub(1000))
            .event_signature(InteropCenter::InteropMessageSent::SIGNATURE_HASH)
            .address(entry.interop_address);
        let logs = entry.provider.get_logs(&filter).await?;

        let msgs: Vec<_> = logs
            .iter()
            .map(|l| InteropMessageParsed::from_log(l))
            .collect();

        println!("Got {} msgs", msgs.len());

        for m in msgs {
            let is_type_b = m.is_type_b();
            dbg!(is_type_b);
        }
    }

    // We have to support 2 things:
    // * for each 'interop message' - 'deliver' it to all the other locations
    // * for each 'type C' message - detect, create a payload and send.

    /*
    let signer: PrivateKeySigner = PrivateKeySigner::from_signing_key(
        // private key from account 7.
        SigningKey::from_bytes(
            &hex!("4d91647d0a8429ac4433c83254fb9625332693c848e578062fe96362f32bfe91").into(),
        )
        .unwrap(),
    );

    if false {
        let wallet = ZksyncWallet::from(signer);

        // Create a provider with the wallet.
        let provider = zksync_provider()
            .with_recommended_fillers()
            .wallet(wallet)
            .on_http("http://localhost:8011".parse().unwrap());

        // Build a transaction to send 100 wei from Alice to Vitalik.
        // The `from` field is automatically filled to the first signer's address (Alice).
        let tx = TransactionRequest::default()
            .with_to(address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"))
            .with_value(U256::from(100))
            .with_from(address!("42F3dc38Da81e984B92A95CBdAAA5fA2bd5cb1Ba"));

        // Send the transaction and wait for inclusion.
        let receipt = provider.send_transaction(tx).await?.get_receipt().await?;
        println!("Got receipt: {receipt:#?}");
    }

    if false {
        let other_provider = zksync_provider()
            .with_recommended_fillers()
            .on_http("http://localhost:8011".parse().unwrap());

        // Build a transaction to send 100 wei from Alice to Vitalik.
        // The `from` field is automatically filled to the first signer's address (Alice).
        let mut tx = TransactionRequest::default()
            .with_to(address!("d8dA6BF26964aF9D7eEd9e03E53415D37aA96045"))
            .with_value(U256::from(100))
            .with_gas_limit(10_000_000)
            .with_gas_per_pubdata(U256::from(50_000))
            .with_max_fee_per_gas(100_000_000)
            .with_max_priority_fee_per_gas(100_000_000)
            .with_from(address!("7e24c9C86368159be470008a0F0d5df28612ca2b"));

        println!("{:?}", tx.output_tx_type());

        //let mut foo = tx.clone();

        TransactionBuilder::prep_for_submission(&mut tx);

        // Send the transaction and wait for inclusion.
        let receipt = other_provider
            .send_transaction(tx)
            .await?
            .get_receipt()
            .await?;
        println!("Got second receipt: {receipt:#?}");
    }

    // Call `zks` namespace RPC.
    //let l1_chain_id = provider.get_l1_chain_id().await?;
    // println!("L1 chain ID is: {l1_chain_id}");

    */
    Ok(())
}
