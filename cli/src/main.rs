use std::any::Any;

use alloy::{
    hex,
    network::TransactionBuilder,
    primitives::{address, U256},
    providers::Provider,
    signers::local::PrivateKeySigner,
};
use alloy_zksync::{
    network::transaction_request::TransactionRequest,
    provider::{zksync_provider, ZksyncProvider},
    wallet::ZksyncWallet,
};
use k256::ecdsa::SigningKey;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    {
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

    Ok(())
}
