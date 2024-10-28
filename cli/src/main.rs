use alloy::{
    dyn_abi::{DynSolType, DynSolValue, SolType},
    hex,
    network::TransactionBuilder,
    primitives::{address, Address, Bytes, FixedBytes, B256, U256},
    providers::Provider,
    rpc::types::{Filter, Log},
    signers::local::PrivateKeySigner,
    sol_types::{SolCall, SolEvent},
};
use alloy_zksync::{
    network::transaction_request::TransactionRequest,
    provider::{self, zksync_provider},
    wallet::ZksyncWallet,
};
use k256::ecdsa::SigningKey;

use alloy::sol;

use clap::Parser;
use std::{collections::HashMap, hash::Hash, marker::PhantomData, str::FromStr};
use InteropCenter::InteropMessage;

sol! {
    #[sol(rpc)]
    contract InteropCenter {
        event InteropMessageSent(
            bytes32 indexed msgHash,
            address indexed sender,
            bytes payload
        );


        struct InteropMessage {
            bytes data;
            address sender;
            uint256 sourceChainId;
            uint256 messageNum;
        }

        struct InteropTransaction {
            address sourceChainSender;
            uint256 destinationChain;
            uint256 gasLimit;
            uint256 gasPrice;
            uint256 value;
            bytes32 bundleHash;
            bytes32 feesBundleHash;
            address destinationPaymaster;
            bytes destinationPaymasterInput;
        }

        function getAliasedAccount(
            address sourceAccount,
            uint256 sourceChainId
        ) public returns (address);


        function executeInteropBundle(
            InteropMessage memory message,
            bytes memory proof
        ) public ;


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

    pub interop_message: InteropCenter::InteropMessage,
}

impl InteropMessageParsed {
    pub fn from_log(log: &Log) -> Self {
        let interop_message =
            InteropCenter::InteropMessage::abi_decode(&log.data().data.slice(64..), true).unwrap();

        InteropMessageParsed {
            interop_center_sender: log.address(),
            msg_hash: log.topics()[1],
            sender: Address::from_slice(&log.topics()[2].0[12..]),
            data: log.data().data.clone(),
            interop_message,
        }
    }

    pub fn is_type_b(&self) -> bool {
        return self.interop_center_sender == self.sender && self.interop_message.data[0] == 1;
    }
    pub fn is_type_c(&self) -> bool {
        return self.interop_center_sender == self.sender && self.interop_message.data[0] == 2;
    }

    pub async fn create_transaction_request(
        &self,
        providers_map: &HashMap<u64, InteropChain>,
        all_messages: &HashMap<FixedBytes<32>, InteropMessage>,
    ) -> TransactionRequest {
        let interop_tx =
            InteropCenter::InteropTransaction::abi_decode(&self.interop_message.data[1..], true)
                .unwrap();

        println!("interop tx desxtination: {}", interop_tx.destinationChain);
        let destination_interop_chain = providers_map
            .get(&interop_tx.destinationChain.try_into().unwrap())
            .unwrap();

        let from_addr = destination_interop_chain
            .get_aliased_account_address(
                // TODO: Or provided from the RPC??
                self.interop_message.sourceChainId,
                interop_tx.sourceChainSender,
            )
            .await;

        println!("Got 'from' address set to: {:?}", from_addr);

        let bundle_msg = all_messages.get(&interop_tx.bundleHash).unwrap();

        let proof = Bytes::new();

        let calldata = InteropCenter::executeInteropBundleCall::new((bundle_msg.clone(), proof));

        // TODO: add calldata too.

        TransactionRequest::default()
            .with_call(&calldata)
            .with_to(destination_interop_chain.interop_address)
            .with_value(interop_tx.value)
            .with_gas_limit(interop_tx.gasLimit.try_into().unwrap())
            .with_gas_per_pubdata(U256::from(50_000))
            .with_max_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_max_priority_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_from(from_addr)
    }

    // Checks if the interop message is of type b.
}

struct InteropChain {
    pub provider: alloy::providers::fillers::FillProvider<
        alloy::providers::fillers::JoinFill<
            alloy::providers::Identity,
            alloy::providers::fillers::JoinFill<
                alloy_zksync::provider::fillers::Eip712FeeFiller,
                alloy::providers::fillers::JoinFill<
                    alloy::providers::fillers::NonceFiller,
                    alloy::providers::fillers::ChainIdFiller,
                >,
            >,
        >,
        alloy::providers::RootProvider<
            alloy::transports::http::Http<reqwest::Client>,
            alloy_zksync::network::Zksync,
        >,
        alloy::transports::http::Http<reqwest::Client>,
        alloy_zksync::network::Zksync,
    >,
    pub interop_address: Address,
    pub rpc: String,
}

impl InteropChain {
    pub async fn get_aliased_account_address(
        &self,
        source_chain: U256,
        source_address: Address,
    ) -> Address {
        println!("Calling {:?}", self.interop_address);
        println!("params {:?} {:?}", source_address, source_chain);

        let contract = InteropCenter::new(self.interop_address, &self.provider);
        contract
            .getAliasedAccount(source_address, source_chain)
            .call()
            .await
            .unwrap()
            ._0
    }
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

    let mut all_messages = HashMap::new();

    for (_, entry) in &providers_map {
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

        for m in &msgs {
            all_messages.insert(m.msg_hash, m.interop_message.clone());
        }

        println!("Got {} msgs", msgs.len());

        for m in msgs {
            let is_type_b = m.is_type_b();
            dbg!(is_type_b);
            let is_type_c = m.is_type_c();
            dbg!(is_type_c);

            if is_type_c {
                let tx = m
                    .create_transaction_request(&providers_map, &all_messages)
                    .await;

                let receipt = entry
                    .provider
                    .send_transaction(tx)
                    .await?
                    .get_receipt()
                    .await?;
                println!("Got receipt: {receipt:#?}");
            }
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
