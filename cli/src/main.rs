use alloy::{
    dyn_abi::SolType,
    hex::FromHex,
    network::TransactionBuilder,
    primitives::{address, Address, Bytes, FixedBytes, U256},
    providers::Provider,
    rpc::types::{Filter, Log},
    signers::local::PrivateKeySigner,
    sol_types::{SolCall, SolEvent},
};
use alloy_zksync::{
    network::{transaction_request::TransactionRequest, unsigned_tx::eip712::PaymasterParams},
    provider::zksync_provider,
    wallet::ZksyncWallet,
};
use k256::ecdsa::SigningKey;

use alloy::sol;

use clap::Parser;
use futures_util::stream::StreamExt;
use std::{
    collections::HashMap,
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
};
use tokio::sync::Mutex;

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
        ) public;


        function receiveInteropMessage(bytes32 msgHash) public;

        function addTrustedSource(
            uint256 sourceChainId,
            address trustedSender
        ) public;

        function deployAliasedAccount(
            address sourceAccount,
            uint256 sourceChainId
        ) public returns (address);

        mapping(uint256 => address) public preferredPaymasters;

    }
}

pub struct InteropMessageParsed {
    pub interop_center_sender: Address,
    // The unique global identifier of this message.
    pub msg_hash: FixedBytes<32>,
    // The address that sent this message on the source chain.
    pub sender: Address,

    // 'data' field from the Log (it contains the InteropMessage).
    pub data: Bytes,

    pub interop_message: InteropCenter::InteropMessage,
    pub chain_id: u64,
}

impl Debug for InteropMessageParsed {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InteropMessageParsed")
            .field("interop_center_sender", &self.interop_center_sender)
            .field("msg_hash", &self.msg_hash)
            .field("sender", &self.sender)
            .field("data", &self.data)
            .finish()
    }
}

impl InteropMessageParsed {
    pub fn from_log(log: &Log, chain_id: u64) -> Self {
        let interop_message =
            InteropCenter::InteropMessage::abi_decode(&log.data().data.slice(64..), true).unwrap();

        InteropMessageParsed {
            interop_center_sender: log.address(),
            msg_hash: log.topics()[1],
            sender: Address::from_slice(&log.topics()[2].0[12..]),
            data: log.data().data.clone(),
            interop_message,
            chain_id,
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
        providers_map: &HashMap<u64, Arc<InteropChain>>,
        all_messages: Arc<Mutex<HashMap<FixedBytes<32>, InteropMessageParsed>>>,
    ) -> (u64, TransactionRequest) {
        let interop_tx =
            InteropCenter::InteropTransaction::abi_decode(&self.interop_message.data[1..], true)
                .unwrap();

        println!("interop tx desxtination: {}", interop_tx.destinationChain);

        let destination_chain_id: u64 = interop_tx.destinationChain.try_into().unwrap();
        let destination_interop_chain = providers_map.get(&destination_chain_id).unwrap();

        let from_addr = destination_interop_chain
            .get_aliased_account_address(
                // TODO: Or provided from the RPC??
                self.interop_message.sourceChainId,
                interop_tx.sourceChainSender,
            )
            .await;

        println!("Got 'from' address set to: {:?}", from_addr);

        let code = destination_interop_chain
            .provider
            .get_code_at(from_addr)
            .await
            .unwrap();
        println!("Code length is {}", code.len());
        if code.len() == 0 {
            // No contract deployed.

            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_interop_chain.admin_wallet.clone())
                .on_http(destination_interop_chain.rpc.parse().unwrap());

            let contract =
                InteropCenter::new(destination_interop_chain.interop_address, &admin_provider);

            // TODO: before sending, maybe check if the message was forwarded already..

            let tx_hash = contract
                .deployAliasedAccount(
                    interop_tx.sourceChainSender,
                    self.interop_message.sourceChainId,
                )
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();

            println!(
                "Deployed aliased account on chain {} {:?} with tx {:?}",
                destination_chain_id, from_addr, tx_hash
            );
        }

        // TODO: take this from the interop_tx instead.
        let paymaster_params = PaymasterParams {
            paymaster: destination_interop_chain.get_preferred_paymaster().await,
            paymaster_input: Bytes::from_hex("0x1234").unwrap(),
        };
        let paymaster = paymaster_params.paymaster;
        println!("Using paymaster: {}", paymaster);

        let balance = destination_interop_chain
            .provider
            .get_balance(paymaster)
            .await
            .unwrap();

        // Refill the paymaster if needed.
        let mut limit: U256 = 1_000_000.try_into().unwrap();
        limit = limit.checked_mul(1_000_000.try_into().unwrap()).unwrap();
        limit = limit.checked_mul(1_000_000.try_into().unwrap()).unwrap();

        if balance < limit {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_interop_chain.admin_wallet.clone())
                .on_http(destination_interop_chain.rpc.parse().unwrap());
            let tx = TransactionRequest::default()
                .with_to(paymaster)
                .with_value(limit);
            let tx_hash = admin_provider
                .send_transaction(tx)
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();
            println!("Sending 1 eth : {:?} ", tx_hash);
        }

        let map = all_messages.lock().await;

        let bundle_msg = map.get(&interop_tx.bundleHash).unwrap();

        let proof = Bytes::new();

        let calldata = InteropCenter::executeInteropBundleCall::new((
            bundle_msg.interop_message.clone(),
            proof,
        ));

        let tx = TransactionRequest::default()
            .with_call(&calldata)
            .with_to(destination_interop_chain.interop_address)
            // FIXME: no value passing.
            //.with_value(interop_tx.value)
            .with_gas_limit(interop_tx.gasLimit.try_into().unwrap())
            .with_gas_per_pubdata(U256::from(50_000))
            .with_max_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_max_priority_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_from(from_addr)
            .with_paymaster(paymaster_params);

        (destination_chain_id, tx)
    }

    // Checks if the interop message is of type b.
}

#[derive(Clone)]
pub struct InteropChain {
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
    pub chain_id: u64,
    pub admin_wallet: ZksyncWallet,
}

const BLOCKS_IN_THE_PAST: u64 = 1000;

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

    pub async fn get_preferred_paymaster(&self) -> Address {
        let contract = InteropCenter::new(self.interop_address, &self.provider);
        contract
            .preferredPaymasters(self.chain_id.try_into().unwrap())
            .call()
            .await
            .unwrap()
            ._0
    }

    pub async fn listen_on_interop_messages<F, Fut>(&self, callback: F)
    where
        F: Fn(Log) -> Fut,
        Fut: futures::future::Future<Output = ()>,
    {
        let latest_block = self.provider.get_block_number().await.unwrap();

        // Look at last 1k blocks.
        let filter = Filter::new()
            .from_block(latest_block.saturating_sub(BLOCKS_IN_THE_PAST))
            .event_signature(InteropCenter::InteropMessageSent::SIGNATURE_HASH)
            .address(self.interop_address);

        let logs = self.provider.get_logs(&filter).await.unwrap();

        for log in logs {
            callback(log).await;
        }

        println!("Starting to watch logs...");

        let mut log_stream = self
            .provider
            .watch_logs(&filter)
            .await
            .unwrap()
            .into_stream();

        while let Some(log) = log_stream.next().await {
            for l in log {
                callback(l).await;
            }
        }
    }
}

async fn handle_type_a_message(
    msg: &InteropMessageParsed,
    providers_map: &HashMap<u64, Arc<InteropChain>>,
) {
    // Forward the message to all the other chains.
    for (chain_id, entry) in providers_map {
        let admin_provider = zksync_provider()
            .with_recommended_fillers()
            .wallet(entry.admin_wallet.clone())
            .on_http(entry.rpc.parse().unwrap());

        let contract = InteropCenter::new(entry.interop_address, &admin_provider);

        // TODO: before sending, maybe check if the message was forwarded already..

        let tx_hash = contract
            .receiveInteropMessage(msg.msg_hash)
            .send()
            .await
            .unwrap()
            .watch()
            .await
            .unwrap();

        println!("Forwarded msg to {} with tx {:?}", chain_id, tx_hash);
    }
}

async fn handle_type_c_message(
    msg: &InteropMessageParsed,
    providers_map: &HashMap<u64, Arc<InteropChain>>,
    shared_map: Arc<Mutex<HashMap<FixedBytes<32>, InteropMessageParsed>>>,
) {
    // TODO: also create the aliased account if needed...

    let (destination_chain, tx) = msg
        .create_transaction_request(providers_map, shared_map.clone())
        .await;
    let receipt = providers_map
        .get(&destination_chain)
        .unwrap()
        .provider
        .send_transaction(tx)
        .await
        .unwrap()
        .get_receipt()
        .await
        .unwrap();

    println!(
        "Sent type C tx to: {} hash: {}",
        destination_chain, receipt.inner.transaction_hash
    )
}

#[derive(Parser, Debug)]
#[command(name = "Ethereum Interop CLI")]
#[command(version = "1.0")]
#[command(about = "Handles RPC URLs and interop Ethereum addresses")]
struct Cli {
    /// List of RPC URL and interop address pairs (e.g. -r URL ADDRESS)
    #[arg(short, long, num_args = 2, value_names = ["URL", "ADDRESS"])]
    rpc: Vec<String>,

    #[arg(long)]
    private_key: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let mut rpc_addresses = Vec::new();

    let private_key = cli
        .private_key
        .strip_prefix("0x")
        .unwrap_or(&cli.private_key);

    let signer: PrivateKeySigner = PrivateKeySigner::from_signing_key(
        // private key from account 7.
        SigningKey::from_bytes(Vec::from_hex(private_key).unwrap().as_slice().into()).unwrap(),
    );
    let admin_wallet = ZksyncWallet::from(signer);

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
            Arc::new(InteropChain {
                provider,
                interop_address,
                rpc: rpc.clone(),
                chain_id,
                admin_wallet: admin_wallet.clone(),
            }),
        );
        if let Some(prev) = prev {
            panic!(
                "Two interops with the same chain id {} -- {} and {} ",
                chain_id, rpc, prev.rpc
            );
        }
    }
    for (_, source_chain) in &providers_map {
        for (_, destination_chain) in &providers_map {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_chain.admin_wallet.clone())
                .on_http(destination_chain.rpc.parse().unwrap());

            let contract = InteropCenter::new(destination_chain.interop_address, &admin_provider);

            // TODO: before sending, maybe check if trust was already set?

            let tx_hash = contract
                .addTrustedSource(
                    source_chain.chain_id.try_into().unwrap(),
                    source_chain.interop_address,
                )
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();

            println!(
                "Trust from {} to {} tx {:?}",
                source_chain.chain_id, destination_chain.chain_id, tx_hash
            );
        }
    }

    let shared_map: Arc<Mutex<HashMap<FixedBytes<32>, InteropMessageParsed>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let handles: Vec<_> = providers_map
        .clone()
        .iter()
        .map(|(_, entry)| {
            let entry2 = Arc::clone(&entry);
            println!("Starting the spawn..");
            let shared_map = shared_map.clone();
            let providers_map = providers_map.clone();
            let handle = tokio::task::spawn(async move {
                let shared_map = shared_map.clone();
                let chain_id = entry2.chain_id;
                entry2
                    .listen_on_interop_messages(|log| {
                        let shared_map = shared_map.clone();
                        let providers_map = providers_map.clone();
                        let chain_id = chain_id.clone();
                        async move {
                            let msg = InteropMessageParsed::from_log(&log, chain_id);

                            println!("Got msg {:?}", msg);

                            handle_type_a_message(&msg, &providers_map).await;

                            if msg.is_type_c() {
                                handle_type_c_message(&msg, &providers_map, shared_map.clone())
                                    .await;
                            }
                            let mut map = shared_map.lock().await;

                            map.insert(msg.msg_hash, msg);
                        }
                    })
                    .await;
            });
            handle
        })
        .collect();

    futures::future::join_all(handles).await;

    // We have to support 2 things:
    // * for each 'interop message' - 'deliver' it to all the other locations
    // * for each 'type C' message - detect, create a payload and send.

    Ok(())
}
