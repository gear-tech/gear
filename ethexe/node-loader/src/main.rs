mod args;
mod batch;
mod utils;
use alloy::{
    network::Network,
    primitives::Address,
    providers::{Provider, RootProvider},
};
use anyhow::Result;
use args::{Params, parse_cli_params};
use ethexe_common::k256::ecdsa::SigningKey;
use ethexe_ethereum::Ethereum;

use ethexe_signer::{KeyStorage, MemoryKeyStorage};
use rand::rngs::SmallRng;
use std::str::FromStr;
use tokio::sync::broadcast::Sender;
use tracing::info;

use crate::{args::LoadParams, batch::BatchPool};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let params = parse_cli_params();

    match params {
        Params::Dump { seed } => {
            info!("Dump requested with seed: {}", seed);
            // Dump logic would go here
            Ok(())
        }
        Params::Load(load_params) => {
            info!("Starting load test on {}", load_params.node);

            load_node(load_params).await
        }
    }
}

async fn load_node(params: LoadParams) -> Result<()> {
    const MAX_WORKERS: usize = 48;
    const MINT_AMOUNT: u128 = 500_000_000_000_000_000_000;
    const DEPLOYER_ADDRESS: &str = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
    const DEPLOYER_PRIVATE_KEY: &str =
        "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80";

    if params.workers == 0 {
        return Err(anyhow::anyhow!("workers must be greater than 0"));
    }

    if params.workers > MAX_WORKERS {
        return Err(anyhow::anyhow!("workers must not exceed {MAX_WORKERS}"));
    }

    let router_addr = Address::from_str(&params.router_address).unwrap();

    let accounts = prefunded_accounts();
    if params.workers > accounts.len() {
        return Err(anyhow::anyhow!(
            "workers must not exceed available accounts ({})",
            accounts.len()
        ));
    }

    let deployer_signing_key = alloy::hex::decode(DEPLOYER_PRIVATE_KEY).unwrap();
    let mut deployer_keystore = MemoryKeyStorage::empty();
    let deployer_signing_key =
        SigningKey::from_slice(deployer_signing_key.as_ref()).expect("Invalid deployer key");
    let deployer_pubkey = deployer_keystore
        .add_key(deployer_signing_key.into())
        .unwrap();
    let deployer_signer = ethexe_signer::Signer::new(deployer_keystore);
    let deployer_expected_address = Address::from_str(DEPLOYER_ADDRESS).unwrap();
    if deployer_pubkey.to_address().0 != deployer_expected_address.0.0 {
        return Err(anyhow::anyhow!(
            "deployer address mismatch: expected {deployer_expected_address:?}, got {:?}",
            deployer_pubkey.to_address()
        ));
    }

    let deployer_api = Ethereum::new(
        &params.node,
        router_addr.into(),
        deployer_signer,
        deployer_pubkey.to_address(),
    )
    .await?;

    let mut apis = Vec::with_capacity(params.workers);
    for account in accounts.into_iter().take(params.workers) {
        let signing_key = alloy::hex::decode(account.private_key).unwrap();

        let mut keystore = MemoryKeyStorage::empty();
        let signing_key =
            SigningKey::from_slice(signing_key.as_ref()).expect("Invalid signing key");
        let pubkey = keystore.add_key(signing_key.into()).unwrap();
        let signer = ethexe_signer::Signer::new(keystore);

        let expected_address = Address::from_str(account.address).unwrap();
        if pubkey.to_address().0 != expected_address.0.0 {
            return Err(anyhow::anyhow!(
                "prefunded account address mismatch: expected {expected_address:?}, got {:?}",
                pubkey.to_address()
            ));
        }

        let api = Ethereum::new(
            &params.node,
            router_addr.into(),
            signer,
            pubkey.to_address(),
        )
        .await?;

        deployer_api
            .wrapped_vara()
            .mint(pubkey.to_address(), MINT_AMOUNT)
            .await?;
        api.wrapped_vara().approve_all(pubkey.to_address()).await?;

        apis.push(api);
    }

    let provider = apis
        .first()
        .expect("workers must be greater than 0")
        .provider()
        .clone();

    // proportionally increase the channel size to workers and batch size
    // so that we can keep up with the load.
    let (tx, rx) = tokio::sync::broadcast::channel(params.batch_size * params.workers * 48);

    let batch_pool = BatchPool::<SmallRng>::new(
        apis,
        params.ethexe_node.clone(),
        params.workers,
        params.batch_size,
        rx.resubscribe(),
    );

    let run_result = tokio::select! {
        r = listen_blocks(tx, provider.root().clone()) => r,
        r = batch_pool.run(params, rx) => r,
    };

    run_result
}

#[derive(Clone, Copy)]
struct PrefundedAccount {
    address: &'static str,
    private_key: &'static str,
}

fn prefunded_accounts() -> Vec<PrefundedAccount> {
    vec![
        PrefundedAccount {
            address: "0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc",
            private_key: "0x5de4111afa1a4b94908f83103eb1f1706367c2e68ca870fc3fb9a804cdab365a",
        },
        PrefundedAccount {
            address: "0x90f79bf6eb2c4f870365e785982e1f101e93b906",
            private_key: "0x7c852118294e51e653712a81e05800f419141751be58f605c371e15141b007a6",
        },
        PrefundedAccount {
            address: "0x15d34aaf54267db7d7c367839aaf71a00a2c6a65",
            private_key: "0x47e179ec197488593b187f80a00eb0da91f1b9d0b13f8733639f19c30a34926a",
        },
        PrefundedAccount {
            address: "0x9965507d1a55bcc2695c58ba16fb37d819b0a4dc",
            private_key: "0x8b3a350cf5c34c9194ca85829a2df0ec3153be0318b5e2d3348e872092edffba",
        },
        PrefundedAccount {
            address: "0x976ea74026e726554db657fa54763abd0c3a0aa9",
            private_key: "0x92db14e403b83dfe3df233f83dfa3a0d7096f21ca9b0d6d6b8d88b2b4ec1564e",
        },
        PrefundedAccount {
            address: "0x14dc79964da2c08b23698b3d3cc7ca32193d9955",
            private_key: "0x4bbbf85ce3377467afe5d46f804f221813b2bb87f24d81f60f1fcdbf7cbf4356",
        },
        PrefundedAccount {
            address: "0x23618e81e3f5cdf7f54c3d65f7fbc0abf5b21e8f",
            private_key: "0xdbda1821b80551c9d65939329250298aa3472ba22feea921c0cf5d620ea67b97",
        },
        PrefundedAccount {
            address: "0xa0ee7a142d267c1f36714e4a8f75612f20a79720",
            private_key: "0x2a871d0798f97d79848a013d4936a73bf4cc922c825d33c1cf7073dff6d409c6",
        },
        PrefundedAccount {
            address: "0xbcd4042de499d14e55001ccbb24a551f3b954096",
            private_key: "0xf214f2b2cd398c806f84e317254e0f0b801d0643303237d97a22a48e01628897",
        },
        PrefundedAccount {
            address: "0x71be63f3384f5fb98995898a86b02fb2426c5788",
            private_key: "0x701b615bbdfb9de65240bc28bd21bbc0d996645a3dd57e7b12bc2bdf6f192c82",
        },
        PrefundedAccount {
            address: "0xfabb0ac9d68b0b445fb7357272ff202c5651694a",
            private_key: "0xa267530f49f8280200edf313ee7af6b827f2a8bce2897751d06a843f644967b1",
        },
        PrefundedAccount {
            address: "0x1cbd3b2770909d4e10f157cabc84c7264073c9ec",
            private_key: "0x47c99abed3324a2707c28affff1267e45918ec8c3f20b8aa892e8b065d2942dd",
        },
        PrefundedAccount {
            address: "0xdf3e18d64bc6a983f673ab319ccae4f1a57c7097",
            private_key: "0xc526ee95bf44d8fc405a158bb884d9d1238d99f0612e9f33d006bb0789009aaa",
        },
        PrefundedAccount {
            address: "0xcd3b766ccdd6ae721141f452c550ca635964ce71",
            private_key: "0x8166f546bab6da521a8369cab06c5d2b9e46670292d85c875ee9ec20e84ffb61",
        },
        PrefundedAccount {
            address: "0x2546bcd3c84621e976d8185a91a922ae77ecec30",
            private_key: "0xea6c44ac03bff858b476bba40716402b03e41b8e97e276d1baec7c37d42484a0",
        },
        PrefundedAccount {
            address: "0xbda5747bfd65f08deb54cb465eb87d40e51b197e",
            private_key: "0x689af8efa8c651a91ad287602527f3af2fe9f6501a7ac4b061667b5a93e037fd",
        },
        PrefundedAccount {
            address: "0xdd2fd4581271e230360230f9337d5c0430bf44c0",
            private_key: "0xde9be858da4a475276426320d5e9262ecfc3ba460bfac56360bfa6c4c28b4ee0",
        },
        PrefundedAccount {
            address: "0x8626f6940e2eb28930efb4cef49b2d1f2c9c1199",
            private_key: "0xdf57089febbacf7ba0bc227dafbffa9fc08a93fdc68e1e42411a14efcf23656e",
        },
        PrefundedAccount {
            address: "0x09db0a93b389bef724429898f539aeb7ac2dd55f",
            private_key: "0xeaa861a9a01391ed3d587d8a5a84ca56ee277629a8b02c22093a419bf240e65d",
        },
        PrefundedAccount {
            address: "0x02484cb50aac86eae85610d6f4bf026f30f6627d",
            private_key: "0xc511b2aa70776d4ff1d376e8537903dae36896132c90b91d52c1dfbae267cd8b",
        },
        PrefundedAccount {
            address: "0x08135da0a343e492fa2d4282f2ae34c6c5cc1bbe",
            private_key: "0x224b7eb7449992aac96d631d9677f7bf5888245eef6d6eeda31e62d2f29a83e4",
        },
        PrefundedAccount {
            address: "0x5e661b79fe2d3f6ce70f5aac07d8cd9abb2743f1",
            private_key: "0x4624e0802698b9769f5bdb260a3777fbd4941ad2901f5966b854f953497eec1b",
        },
        PrefundedAccount {
            address: "0x61097ba76cd906d2ba4fd106e757f7eb455fc295",
            private_key: "0x375ad145df13ed97f8ca8e27bb21ebf2a3819e9e0a06509a812db377e533def7",
        },
        PrefundedAccount {
            address: "0xdf37f81daad2b0327a0a50003740e1c935c70913",
            private_key: "0x18743e59419b01d1d846d97ea070b5a3368a3e7f6f0242cf497e1baac6972427",
        },
        PrefundedAccount {
            address: "0x553bc17a05702530097c3677091c5bb47a3a7931",
            private_key: "0xe383b226df7c8282489889170b0f68f66af6459261f4833a781acd0804fafe7a",
        },
        PrefundedAccount {
            address: "0x87bdce72c06c21cd96219bd8521bdf1f42c78b5e",
            private_key: "0xf3a6b71b94f5cd909fb2dbb287da47badaa6d8bcdc45d595e2884835d8749001",
        },
        PrefundedAccount {
            address: "0x40fc963a729c542424cd800349a7e4ecc4896624",
            private_key: "0x4e249d317253b9641e477aba8dd5d8f1f7cf5250a5acadd1229693e262720a19",
        },
        PrefundedAccount {
            address: "0x9dcce783b6464611f38631e6c851bf441907c710",
            private_key: "0x233c86e887ac435d7f7dc64979d7758d69320906a0d340d2b6518b0fd20aa998",
        },
        PrefundedAccount {
            address: "0x1bcb8e569eedab4668e55145cfeaf190902d3cf2",
            private_key: "0x85a74ca11529e215137ccffd9c95b2c72c5fb0295c973eb21032e823329b3d2d",
        },
        PrefundedAccount {
            address: "0x8263fce86b1b78f95ab4dae11907d8af88f841e7",
            private_key: "0xac8698a440d33b866b6ffe8775621ce1a4e6ebd04ab7980deb97b3d997fc64fb",
        },
        PrefundedAccount {
            address: "0xcf2d5b3cbb4d7bf04e3f7bfa8e27081b52191f91",
            private_key: "0xf076539fbce50f0513c488f32bf81524d30ca7a29f400d68378cc5b1b17bc8f2",
        },
        PrefundedAccount {
            address: "0x86c53eb85d0b7548fea5c4b4f82b4205c8f6ac18",
            private_key: "0x5544b8b2010dbdbef382d254802d856629156aba578f453a76af01b81a80104e",
        },
        PrefundedAccount {
            address: "0x1aac82773cb722166d7da0d5b0fa35b0307dd99d",
            private_key: "0x47003709a0a9a4431899d4e014c1fd01c5aad19e873172538a02370a119bae11",
        },
        PrefundedAccount {
            address: "0x2f4f06d218e426344cfe1a83d53dad806994d325",
            private_key: "0x9644b39377553a920edc79a275f45fa5399cbcf030972f771d0bca8097f9aad3",
        },
        PrefundedAccount {
            address: "0x1003ff39d25f2ab16dbcc18ece05a9b6154f65f4",
            private_key: "0xcaa7b4a2d30d1d565716199f068f69ba5df586cf32ce396744858924fdf827f0",
        },
        PrefundedAccount {
            address: "0x9eaf5590f2c84912a08de97fa28d0529361deb9e",
            private_key: "0xfc5a028670e1b6381ea876dd444d3faaee96cffae6db8d93ca6141130259247c",
        },
        PrefundedAccount {
            address: "0x11e8f3ea3c6fcf12ecff2722d75cefc539c51a1c",
            private_key: "0x5b92c5fe82d4fabee0bc6d95b4b8a3f9680a0ed7801f631035528f32c9eb2ad5",
        },
        PrefundedAccount {
            address: "0x7d86687f980a56b832e9378952b738b614a99dc6",
            private_key: "0xb68ac4aa2137dd31fd0732436d8e59e959bb62b4db2e6107b15f594caf0f405f",
        },
        PrefundedAccount {
            address: "0x9ef6c02fb2ecc446146e05f1ff687a788a8bf76d",
            private_key: "0xc95eaed402c8bd203ba04d81b35509f17d0719e3f71f40061a2ec2889bc4caa7",
        },
        PrefundedAccount {
            address: "0x08a2de6f3528319123b25935c92888b16db8913e",
            private_key: "0x55afe0ab59c1f7bbd00d5531ddb834c3c0d289a4ff8f318e498cb3f004db0b53",
        },
        PrefundedAccount {
            address: "0xe141c82d99d85098e03e1a1cc1cde676556fdde0",
            private_key: "0xc3f9b30f83d660231203f8395762fa4257fa7db32039f739630f87b8836552cc",
        },
        PrefundedAccount {
            address: "0x4b23d303d9e3719d6cdf8d172ea030f80509ea15",
            private_key: "0x3db34a7bcc6424e7eadb8e290ce6b3e1423c6e3ef482dd890a812cd3c12bbede",
        },
        PrefundedAccount {
            address: "0xc004e69c5c04a223463ff32042dd36dabf63a25a",
            private_key: "0xae2daaa1ce8a70e510243a77187d2bc8da63f0186074e4a4e3a7bfae7fa0d639",
        },
        PrefundedAccount {
            address: "0x5eb15c0992734b5e77c888d713b4fc67b3d679a2",
            private_key: "0x5ea5c783b615eb12be1afd2bdd9d96fae56dda0efe894da77286501fd56bac64",
        },
        PrefundedAccount {
            address: "0x7ebb637fd68c523613be51aad27c35c4db199b9c",
            private_key: "0xf702e0ff916a5a76aaf953de7583d128c013e7f13ecee5d701b49917361c5e90",
        },
        PrefundedAccount {
            address: "0x3c3e2e178c69d4bad964568415a0f0c84fd6320a",
            private_key: "0x7ec49efc632757533404c2139a55b4d60d565105ca930a58709a1c52d86cf5d3",
        },
        PrefundedAccount {
            address: "0x35304262b9e87c00c430149f28dd154995d01207",
            private_key: "0x755e273950f5ae64f02096ae99fe7d4f478a28afd39ef2422068ee7304c636c0",
        },
        PrefundedAccount {
            address: "0xd4a1e660c916855229e1712090ccfd8a424a2e33",
            private_key: "0xaf6ecabcdbbfb2aefa8248b19d811234cd95caa51b8e59b6ffd3d4bbc2a6be4c",
        },
    ]
}

async fn listen_blocks(
    tx: Sender<<alloy::network::Ethereum as Network>::HeaderResponse>,
    provider: RootProvider,
) -> Result<()> {
    let mut sub = provider.subscribe_blocks().await?;

    while let Ok(block) = sub.recv().await {
        tx.send(block)
            .map_err(|_| anyhow::anyhow!("Failed to send block"))?;
    }

    todo!()
}
