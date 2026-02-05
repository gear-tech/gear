## ETHEXE (VARA-ETH)

# How to send injected transactions

```rust
    use alloy::{
        eips::BlockId,
        network::Ethereum,
        providers::{Provider as _, ProviderBuilder, RootProvider},
    };
    use ethexe_common::{
        Address,
        injected::{InjectedTransaction, AddressedInjectedTransaction},
    };
    use ethexe_rpc::InjectedClient as _;
    use ethexe_signer::Signer;
    use gprimitives::H256;
    use jsonrpsee::ws_client::WsClientBuilder;
    use std::str::FromStr as _;

    #[tokio::test]
    async fn send_injected_transaction() {
        const VALIDATOR_RPC_URL: &str = "wss://vara-eth-validator-1.gear-tech.io:9944";
        const HOODI_RETH_RPC_URL: &str = "https://hoodi-reth-rpc.gear-tech.io";
        const MIRROR_ADDRESS: &str = "0x3b1cdcBD3D6Fcaf7D1DFAB50479190edf515b5f7";

        let client = WsClientBuilder::new()
            .build(VALIDATOR_RPC_URL)
            .await
            .unwrap();

        let signer = Signer::memory();
        let key = signer.generate_key().unwrap();

        let provider: RootProvider<Ethereum> = ProviderBuilder::default()
            .connect(HOODI_RETH_RPC_URL)
            .await
            .unwrap();
        let reference_block = provider
            .get_block(BlockId::latest())
            .await
            .unwrap()
            .unwrap()
            .hash()
            .0
            .into();

        let tx = InjectedTransaction {
            destination: Address::from_str(MIRROR_ADDRESS).unwrap().into(),
            payload: b"PING".to_vec().into(),
            value: 0,
            reference_block,
            salt: H256::random().0.to_vec().into(),
        };

        let transaction = AddressedInjectedTransaction {
            recipient: Address::default(),
            tx: signer.signed_data(key, tx).unwrap(),
        };

        println!("Sending transaction...");

        let mut s = client
            .send_transaction_and_watch(transaction)
            .await
            .unwrap();

        println!("Waiting for promise...");

        let promise = s
            .next()
            .await
            .expect("promise from subscription")
            .expect("transaction promise")
            .into_data();

        println!("Promise: {:?}", promise);
    }
```