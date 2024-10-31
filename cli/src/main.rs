use alloy::{
    consensus::Signed,
    dyn_abi::SolType,
    hex::FromHex,
    network::TransactionBuilder,
    primitives::{Address, Bytes, FixedBytes, B256, U256},
    providers::Provider,
    rlp::BytesMut,
    rpc::types::{Filter, Log},
    signers::local::PrivateKeySigner,
    sol_types::{SolCall, SolEvent},
};
use alloy_zksync::{
    network::{
        transaction_request::TransactionRequest, tx_envelope::TxEnvelope,
        unsigned_tx::eip712::PaymasterParams,
    },
    provider::zksync_provider,
    wallet::ZksyncWallet,
};

use alloy::network::eip2718::Encodable2718;

use alloy::signers::Signature;
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
        ) public;


        function receiveInteropMessage(bytes32 msgHash) public;
        mapping(bytes32 => bool) public receivedMessages;


        function addTrustedSource(
            uint256 sourceChainId,
            address trustedSender
        ) public;
        mapping(uint256 => address) public trustedSources;


        function deployAliasedAccount(
            address sourceAccount,
            uint256 sourceChainId
        ) public returns (address);

        mapping(uint256 => address) public preferredPaymasters;

        function setPreferredPaymaster(
            uint256 chainId,
            address paymaster
        ) public;

        mapping(bytes32 => bool) public executedBundles;



        struct TransactionReservedStuff {
            // For now - figure out of there is a better place for them.

            address sourceChainSender;
            address interopMessageSender;
            uint256 sourceChainId;
            uint256 messageNum;
            uint256 destinationChainId;
            bytes32 bundleHash;
            bytes32 feesBundleHash;
        }


    }

    #[sol(rpc)]
    contract CrossPaymaster {
        address public paymasterTokenAddress;
    }

    #[sol(rpc)]
    contract PaymasterToken {
        mapping(uint256 => address) public remoteAddresses;
        mapping(uint256 => uint256) public ratioNominator;
        mapping(uint256 => uint256) public ratioDenominator;
        function addOtherBridge(
            uint256 sourceChainId,
            address sourceAddress,
            uint256 ratioNominator,
            uint256 ratioDenominator
        ) public;
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
    ) -> Option<(u64, TransactionRequest)> {
        let interop_tx =
            InteropCenter::InteropTransaction::abi_decode(&self.interop_message.data[1..], true)
                .unwrap();

        println!("Interop TX: destination: {}", interop_tx.destinationChain);

        let destination_chain_id: u64 = interop_tx.destinationChain.try_into().unwrap();
        let destination_interop_chain = providers_map.get(&destination_chain_id).unwrap();

        if destination_interop_chain
            .is_bundle_executed(interop_tx.bundleHash)
            .await
        {
            println!("    Bundle is already executed");
            return None;
        }

        if !interop_tx.feesBundleHash.is_zero()
            && destination_interop_chain
                .is_bundle_executed(interop_tx.feesBundleHash)
                .await
        {
            println!("    Fee Bundle is already executed");
            return None;
        }

        let from_addr = destination_interop_chain
            .get_aliased_account_address(
                // TODO: Or provided from the RPC??
                self.interop_message.sourceChainId,
                interop_tx.sourceChainSender,
            )
            .await;

        println!("  'from' address set to: {:?}", from_addr);

        let code = destination_interop_chain
            .provider
            .get_code_at(from_addr)
            .await
            .unwrap();
        if code.len() == 0 {
            // No contract deployed.
            println!("  No account for this user - deploying aliased account.");

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
                "   Deployed aliased account on chain {} {:?} with tx {:?}",
                destination_chain_id, from_addr, tx_hash
            );
        }

        let map = all_messages.lock().await;

        let paymaster_input = if !interop_tx.feesBundleHash.is_zero() {
            println!("  Fee Bundle is present");
            let fee_interop_msg = map
                .get(&interop_tx.feesBundleHash)
                .expect(&format!(
                    "Failed to find fee bundle msg: {:?}",
                    interop_tx.feesBundleHash
                ))
                .interop_message
                .clone();
            InteropMessage::abi_encode(&fee_interop_msg)
        } else {
            vec![]
        };

        let paymaster_params = PaymasterParams {
            paymaster: interop_tx.destinationPaymaster,
            paymaster_input: paymaster_input.into(),
        };
        let paymaster = paymaster_params.paymaster;
        println!("  Using paymaster: {}", paymaster);

        destination_interop_chain.refill_paymaster(paymaster).await;

        let bundle_msg = map.get(&interop_tx.bundleHash).unwrap();

        let proof = Bytes::new();

        let calldata = InteropCenter::executeInteropBundleCall::new((
            bundle_msg.interop_message.clone(),
            proof,
        ));

        let stuff = InteropCenter::TransactionReservedStuff {
            sourceChainSender: interop_tx.sourceChainSender,
            interopMessageSender: self.interop_message.sender,
            sourceChainId: self.interop_message.sourceChainId,
            messageNum: self.interop_message.messageNum,
            destinationChainId: interop_tx.destinationChain,
            bundleHash: interop_tx.bundleHash,
            feesBundleHash: interop_tx.feesBundleHash,
        };

        let custom_signature = InteropCenter::TransactionReservedStuff::abi_encode(&stuff).into();

        let tx = TransactionRequest::default()
            .with_call(&calldata)
            .with_to(destination_interop_chain.interop_address)
            // FIXME: no value passing.
            //.with_value(interop_tx.value)
            .with_gas_limit(interop_tx.gasLimit.try_into().unwrap())
            // Constant for now.
            .with_gas_per_pubdata(U256::from(50_000))
            .with_max_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_max_priority_fee_per_gas(interop_tx.gasPrice.try_into().unwrap())
            .with_from(from_addr)
            .with_paymaster(paymaster_params)
            .with_custom_signature(custom_signature);

        Some((destination_chain_id, tx))
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
    pub base_token_price: u64,
    pub tokens_for_paymaster: U256,
}

const BLOCKS_IN_THE_PAST: u64 = 1000;

impl InteropChain {
    pub async fn get_aliased_account_address(
        &self,
        source_chain: U256,
        source_address: Address,
    ) -> Address {
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

    pub async fn get_paymaster_basic_token(&self) -> Address {
        let paymaster = self.get_preferred_paymaster().await;
        let contract = CrossPaymaster::new(paymaster, &self.provider);
        contract
            .paymasterTokenAddress()
            .call()
            .await
            .unwrap()
            .paymasterTokenAddress
    }

    pub async fn is_bundle_executed(&self, bundle_hash: FixedBytes<32>) -> bool {
        let contract = InteropCenter::new(self.interop_address, &self.provider);
        contract
            .executedBundles(bundle_hash)
            .call()
            .await
            .unwrap()
            ._0
    }

    pub async fn refill_paymaster(&self, paymaster: Address) {
        let balance = self.provider.get_balance(paymaster).await.unwrap();

        // We want 0.02 Eth (roughly).
        let limit = self.tokens_for_paymaster;
        if balance < limit {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(self.admin_wallet.clone())
                .on_http(self.rpc.parse().unwrap());
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
            println!("    Sending {} tokens to paymaster : {:?} ", limit, tx_hash);
        }
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

        if !contract
            .receivedMessages(msg.msg_hash)
            .call()
            .await
            .unwrap()
            ._0
        {
            let tx_hash = contract
                .receiveInteropMessage(msg.msg_hash)
                .send()
                .await
                .unwrap()
                .watch()
                .await
                .unwrap();

            println!("  Forwarded msg to {} with tx {:?}", chain_id, tx_hash);
        }
    }
}

async fn handle_type_c_message(
    msg: &InteropMessageParsed,
    providers_map: &HashMap<u64, Arc<InteropChain>>,
    shared_map: Arc<Mutex<HashMap<FixedBytes<32>, InteropMessageParsed>>>,
) {
    let transaction_request = msg
        .create_transaction_request(providers_map, shared_map.clone())
        .await;

    if let Some((destination_chain, mut tx)) = transaction_request {
        // We do a lot of work here, as era doesn't accept 'eth_sendTransaction' and alloy really wants
        // to sign it with some wallet.
        // So we construct the transaction parts manually - and then send as 'raw' transaction.

        tx.prep_for_submission();
        let provider = &providers_map.get(&destination_chain).unwrap().provider;

        let sendable_tx = provider.fill(tx).await.unwrap();
        let transaction_request = sendable_tx.as_builder().unwrap();

        let unsigned_tx = transaction_request.clone().build_unsigned().unwrap();

        let empty_signature = Signature::new(U256::ZERO, U256::ZERO, Default::default());

        if let alloy_zksync::network::unsigned_tx::TypedTransaction::Eip712(data) = unsigned_tx {
            // What about the hash??
            let signed_tx =
                TxEnvelope::Eip712(Signed::new_unchecked(data, empty_signature, B256::ZERO));

            let mut buffer = BytesMut::new();
            signed_tx.encode_2718(&mut buffer);
            //println!("Transaction payload: {}", hex::encode(&buffer));
            let p1 = provider.send_raw_transaction(&buffer).await;

            match p1 {
                Ok(p1) => {
                    let receipt = p1.get_receipt().await.unwrap();
                    println!(
                        "    === Sent type C tx to: {} hash: {}",
                        destination_chain, receipt.inner.transaction_hash
                    )
                }
                Err(error) => {
                    println!("!! ERROR - {}", error);
                }
            }
        } else {
            panic!("Wrong type");
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "Ethereum Interop CLI")]
#[command(version = "1.0")]
#[command(about = "Handles RPC URLs and interop Ethereum addresses")]
struct Cli {
    /// List of RPC URL and interop address pairs (e.g. -r URL ADDRESS)
    #[arg(short, long, num_args = 2, value_names = ["URL", "ADDRESS"])]
    rpc: Vec<String>,

    // Specify the price of the base token  (10^18) in cents.
    // For eth - you can set it to 200_000.
    #[arg(long)]
    base_token_price: Vec<u64>,

    #[arg(long)]
    private_key: String,

    // How many assets should each paymaster hold. (default ~20USD).
    #[arg(long, default_value = "2000")]
    paymaster_balance_cents: u64,
}

pub fn to_human_size(input: U256) -> String {
    let input = format!("{:?}", input);
    let tmp: Vec<_> = input
        .chars()
        .rev()
        .enumerate()
        .flat_map(|(index, val)| {
            if index > 0 && index % 3 == 0 {
                vec!['_', val]
            } else {
                vec![val]
            }
        })
        .collect();
    tmp.iter().rev().collect()
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
    assert_eq!(
        rpc_addresses.len(),
        cli.base_token_price.len(),
        "Specify as many --base-token-price as --rpc-addresses"
    );

    let mut providers_map = HashMap::new();

    for ((rpc, interop_address), base_token_price) in rpc_addresses
        .into_iter()
        .zip(cli.base_token_price.into_iter())
    {
        let provider = zksync_provider()
            .with_recommended_fillers()
            .on_http(rpc.clone().parse().unwrap());

        let chain_id = provider.get_chain_id().await?;

        let base_token: U256 = 1_000_000_000_000_000_000u64.try_into().unwrap();

        let tokens_for_paymaster = base_token
            .checked_mul(cli.paymaster_balance_cents.try_into().unwrap())
            .unwrap()
            .checked_div(base_token_price.try_into().unwrap())
            .unwrap();

        let prev = providers_map.insert(
            chain_id,
            Arc::new(InteropChain {
                provider,
                interop_address,
                rpc: rpc.clone(),
                chain_id,
                admin_wallet: admin_wallet.clone(),
                base_token_price,
                tokens_for_paymaster,
            }),
        );

        println!(
            "Interop on chain {}. paymaster tokens: {}",
            chain_id,
            to_human_size(tokens_for_paymaster)
        );
        if let Some(prev) = prev {
            panic!(
                "Two interops with the same chain id {} -- {} and {} ",
                chain_id, rpc, prev.rpc
            );
        }
    }

    // Setup trust between inteops.
    for (_, source_chain) in &providers_map {
        for (_, destination_chain) in &providers_map {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_chain.admin_wallet.clone())
                .on_http(destination_chain.rpc.parse().unwrap());

            let contract = InteropCenter::new(destination_chain.interop_address, &admin_provider);

            // TODO: before sending, maybe check if trust was already set?
            let current_trusted_source = contract
                .trustedSources(source_chain.chain_id.try_into().unwrap())
                .call()
                .await
                .unwrap()
                ._0;

            if current_trusted_source != source_chain.interop_address {
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
            } else {
                println!(
                    "Trust from {} to {}  - SKIP",
                    source_chain.chain_id, destination_chain.chain_id
                );
            }
        }
    }

    // Setup info about remote paymasters too
    for (_, source_chain) in &providers_map {
        let source_chain_paymaster = source_chain.get_preferred_paymaster().await;
        for (_, destination_chain) in &providers_map {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_chain.admin_wallet.clone())
                .on_http(destination_chain.rpc.parse().unwrap());

            let contract = InteropCenter::new(destination_chain.interop_address, &admin_provider);

            // TODO: before sending, maybe check if trust was already set?
            let existing_paymaster = contract
                .preferredPaymasters(source_chain.chain_id.try_into().unwrap())
                .call()
                .await
                .unwrap()
                ._0;

            if existing_paymaster != source_chain_paymaster {
                let tx_hash = contract
                    .setPreferredPaymaster(
                        source_chain.chain_id.try_into().unwrap(),
                        source_chain_paymaster,
                    )
                    .send()
                    .await
                    .unwrap()
                    .watch()
                    .await
                    .unwrap();

                println!(
                    "Paymaster Trust from {} to {} tx {:?}",
                    source_chain.chain_id, destination_chain.chain_id, tx_hash
                );
            } else {
                println!(
                    "Paymaster Trust from {} to {} -- SKIP",
                    source_chain.chain_id, destination_chain.chain_id
                );
            }
        }
    }

    // Setup trust between 'paymaster tokens'
    for (_, source_chain) in &providers_map {
        let source_chain_token = source_chain.get_paymaster_basic_token().await;
        for (_, destination_chain) in &providers_map {
            let admin_provider = zksync_provider()
                .with_recommended_fillers()
                .wallet(destination_chain.admin_wallet.clone())
                .on_http(destination_chain.rpc.parse().unwrap());

            let destination_chain_token: Address =
                destination_chain.get_paymaster_basic_token().await;

            let contract = PaymasterToken::new(destination_chain_token, &admin_provider);

            // TODO: before sending, maybe check if trust was already set?

            let existing_source_token = contract
                .remoteAddresses(source_chain.chain_id.try_into().unwrap())
                .call()
                .await
                .unwrap()
                ._0;

            let existing_nominator = contract
                .ratioNominator(source_chain.chain_id.try_into().unwrap())
                .call()
                .await
                .unwrap()
                ._0;

            let existing_denominator = contract
                .ratioDenominator(source_chain.chain_id.try_into().unwrap())
                .call()
                .await
                .unwrap()
                ._0;

            if source_chain_token != existing_source_token
                || existing_nominator != destination_chain.base_token_price.try_into().unwrap()
                || existing_denominator != source_chain.base_token_price.try_into().unwrap()
            {
                let tx_hash = contract
                    .addOtherBridge(
                        source_chain.chain_id.try_into().unwrap(),
                        source_chain_token,
                        destination_chain.base_token_price.try_into().unwrap(),
                        source_chain.base_token_price.try_into().unwrap(),
                    )
                    .send()
                    .await
                    .unwrap()
                    .watch()
                    .await
                    .unwrap();

                println!(
                    "Token Trust from {} to {} tx {:?}",
                    source_chain.chain_id, destination_chain.chain_id, tx_hash
                );
            } else {
                println!(
                    "Token Trust from {} to {} -- SKIP",
                    source_chain.chain_id, destination_chain.chain_id
                );
            }
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

                            println!(
                                "Got msg from chain: {:?} id:{} hash: {:?} ",
                                msg.chain_id, msg.interop_message.messageNum, msg.msg_hash
                            );

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
