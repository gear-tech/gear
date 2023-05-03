use frame_remote_externalities::{Mode, OfflineConfig, RemoteExternalities, SnapshotConfig};
use frame_support::{
    dispatch::{DispatchClass, RawOrigin},
    traits::{Get, OnFinalize, OnInitialize, UpgradeCheckSelect},
};
use frame_system::limits::BlockWeights;
use gear_common::storage::Limiter;
use gear_runtime::{
    pallet_gear::GasAllowanceOf, AccountId, Balances, Block, Executive, Gear, GearGas,
    GearMessenger, Runtime, RuntimeOrigin, System, Weight,
};

type BlockWeightsOf<T> = <T as frame_system::Config>::BlockWeights;

pub const USER_1: AccountId = {
    let mut id = [0; 32];
    id[31] = 1;
    AccountId::new(id)
};

fn new_test_ext_v130() -> RemoteExternalities<Block> {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut ext = frame_remote_externalities::Builder::new()
            .mode(Mode::Offline(OfflineConfig {
                state_snapshot: SnapshotConfig {
                    path: "snapshots/gear-staging-testnet-130.snap".into(),
                },
            }))
            .build()
            .await
            .unwrap();
        ext.execute_with(|| {
            Balances::set_balance(
                RuntimeOrigin::root(),
                USER_1.into(),
                5_000_000_000_000_000_u128,
                0,
            )
            .unwrap();
        });
        ext
    })
}

fn run_to_block(n: u32, remaining_weight: Option<u64>) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        GearGas::on_initialize(System::block_number());
        GearMessenger::on_initialize(System::block_number());
        Gear::on_initialize(System::block_number());

        if let Some(remaining_weight) = remaining_weight {
            GasAllowanceOf::<Runtime>::put(remaining_weight);
            let max_block_weight = <BlockWeightsOf<Runtime> as Get<BlockWeights>>::get().max_block;
            System::register_extra_weight_unchecked(
                max_block_weight.saturating_sub(Weight::from_parts(remaining_weight, 0)),
                DispatchClass::Normal,
            );
        }

        Gear::run(RawOrigin::None.into()).unwrap();
        Gear::on_finalize(System::block_number());

        /*assert!(!System::events().iter().any(|e| {
            matches!(
                e.event,
                RuntimeEvent::Gear(pallet_gear::Event::QueueProcessingReverted)
            )
        }))*/
    }
}

fn main() {
    env_logger::init();
    new_test_ext_v130().execute_with(|| {
        let bn = System::block_number();

        run_to_block(bn + 1, None);

        /*Gear::upload_program(
            RuntimeOrigin::signed(USER_1),
            demo_backend_error::WASM_BINARY.to_vec(),
            vec![],
            vec![],
            1_000_000_000,
            0,
        )
        .unwrap();*/

        //pallet_gear_program::migration::migrate::<Runtime>();

        Executive::try_runtime_upgrade(UpgradeCheckSelect::All).unwrap();

        run_to_block(bn + 2, None);
    });
}
