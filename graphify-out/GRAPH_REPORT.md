# Graph Report - ethexe  (2026-04-07)

## Corpus Check
- 189 files · ~172,488 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 3134 nodes · 5035 edges · 130 communities detected
- Extraction: 60% EXTRACTED · 40% INFERRED · 0% AMBIGUOUS · INFERRED: 1995 edges (avg confidence: 0.5)
- Token cost: 0 input · 0 output

## God Nodes (most connected - your core abstractions)
1. `RawDatabase` - 44 edges
2. `Router<'a>` - 43 edges
3. `RouterQuery` - 34 edges
4. `Mirror<'a>` - 33 edges
5. `NetworkService` - 27 edges
6. `NativeRuntimeInterface` - 27 edges
7. `DatabaseIterator<S>` - 27 edges
8. `MemStorage` - 25 edges
9. `Behaviour` - 23 edges
10. `Behaviour` - 22 edges

## Surprising Connections (you probably didn't know these)
- `main()` --calls--> `load_node()`  [INFERRED]
  ethexe/cli/src/main.rs → ethexe/node-loader/src/main.rs
- `main()` --calls--> `skip_build_on_intellij_sync()`  [INFERRED]
  ethexe/cli/build.rs → ethexe/runtime/build.rs
- `ethexe-node-loader` --conceptually_related_to--> `Ethexe (VARA-ETH)`  [INFERRED]
  ethexe/node-loader/README.md → ethexe/README.md
- `Load Testing Concept` --conceptually_related_to--> `Ethexe (VARA-ETH)`  [INFERRED]
  ethexe/node-loader/README.md → ethexe/README.md

## Hyperedges (group relationships)
- **Injected Transaction Crate Dependencies** — readme_injected_transactions, readme_ethexe_common, readme_ethexe_rpc, readme_gsigner, readme_gprimitives [EXTRACTED 1.00]
- **Node Loader Build Toolchain** — readme_ethexe_node_loader, readme_ethexe_cli, readme_anvil_mnemonic [EXTRACTED 1.00]

## Communities

### Community 0 - "Service Orchestration"
Cohesion: 0.02
Nodes (90): Ext, Behaviour, BehaviourConfig, BlobLoader, BlobLoader<DB>, BlobLoaderError, BlobLoaderEvent, BlobLoaderService (+82 more)

### Community 1 - "Runtime Common & State Migration"
Cohesion: 0.02
Nodes (80): Migration, allocate_and_write(), allocate_and_write_raw(), Behaviour, BlockEvent, BlockRequestEvent, Box<dyn ElectionProvider>, catch_up_3() (+72 more)

### Community 2 - "RPC Block API & Database"
Cohesion: 0.02
Nodes (40): Block, BlockApi, BlockSmallData, Database, dyn CASDatabase + '_, dyn KVDatabase + '_, Key, RawDatabase (+32 more)

### Community 3 - "Runtime Logging"
Cohesion: 0.02
Nodes (58): max_level(), RuntimeLogger, Writer, accept_batch_commitment_validation_reply(), aggregate_code_commitments(), AlternateCollectionFmt, AlternateCollectionFmt<&'a BTreeMap<K, V>>, AlternateCollectionFmt<T> (+50 more)

### Community 4 - "Program State Management"
Cohesion: 0.03
Nodes (23): ActiveProgram, Allocations, Dispatch, DispatchStash, Expiring, Mailbox, MailboxMessage, MemoryPages (+15 more)

### Community 5 - "Ethereum SDK & API"
Cohesion: 0.02
Nodes (36): VaraEthApi, EthereumParams, Event, ExecutableBalanceTopUpRequestedEvent, MessageCallFailedEvent, MessageEvent, MessageQueueingRequestedEvent, Mirror (+28 more)

### Community 6 - "Consensus Core & Middleware"
Cohesion: 0.04
Nodes (34): BatchCommitter, ElectionRequest, MiddlewareWrapper, Router, ValidatorCore, HashOf, MaybeHashOf, accept() (+26 more)

### Community 7 - "Announce Validation"
Cohesion: 0.04
Nodes (35): accept_announce(), AnnounceRejectionReason, AnnounceStatus, base_params(), base_params_and_committed_at(), base_params_and_created_committed_at(), best_announce(), best_parent_announce() (+27 more)

### Community 8 - "Ethereum Event Builders"
Cohesion: 0.03
Nodes (46): AllEventsBuilder, AllEventsBuilder<'a>, AnnounceId, AnnouncesCommittedEventBuilder, AnnouncesCommittedEventBuilder<'a>, ApprovalEventBuilder, ApprovalEventBuilder<'a>, BatchCommittedEventBuilder (+38 more)

### Community 9 - "Batch Commitment Filler"
Cohesion: 0.03
Nodes (31): BatchFiller, BatchIncludeError, AddressBook, AggregatedPublicKey, BatchCommitment, ChainCommitment, CodeCommitment, CodeState (+23 more)

### Community 10 - "Network Connection Slots"
Cohesion: 0.08
Nodes (39): add_inbound_connection_keeps_initial_outbound_direction(), add_inbound_connection_rejects_peer_in_backoff_period(), add_inbound_connection_rejects_when_all_inbound_slots_are_used(), add_inbound_connection_uses_overflowing_slots_after_normal_limit(), add_outbound_connection_allows_multiple_connections_for_known_peer_at_limit(), add_outbound_connection_allows_peer_in_outbound_backoff_period(), add_outbound_connection_allows_reconnect_for_peer_marked_outbound(), add_outbound_connection_keeps_initial_inbound_direction() (+31 more)

### Community 11 - "Kademlia DHT"
Cohesion: 0.07
Nodes (39): add_bootstrap_addresses(), Behaviour, Event, finished_without_additional_record_removes_cached_entry(), get_closest_peers_works(), get_record_cancelled(), get_record_not_found_propagates_error(), get_record_success_is_reported_and_cached() (+31 more)

### Community 12 - "Router Contract Integration"
Cohesion: 0.05
Nodes (5): inexistent_code_is_unknown(), Router, RouterEvents<'a>, RouterQuery, storage_view()

### Community 13 - "Peer Discovery"
Cohesion: 0.08
Nodes (28): Behaviour, behaviour_does_not_query_local_validator_identity(), behaviour_skips_self_query_and_puts(), behaviour_stores_identity_for_known_validator(), Config, different_peer_ids_in_identity(), duplicate_identity_handling(), encode_decode_identity() (+20 more)

### Community 14 - "Integration Tests"
Cohesion: 0.08
Nodes (40): async_and_ping(), block_computation_basic(), call_gr_wait_is_forbidden(), call_wait_up_to_with_huge_duration(), call_wake_with_delay_is_unsupported(), code_validation_request_does_not_block_preparation(), create_new_code(), executable_balance_charged() (+32 more)

### Community 15 - "DB Sync Requests"
Cohesion: 0.06
Nodes (26): AnnouncesResponseError, AnnouncesResponseHandled, HashesResponseError, HashesResponseHandled, make_chain(), OngoingRequest, OngoingRequestContext, OngoingRequests (+18 more)

### Community 16 - "Test Mocks"
Cohesion: 0.04
Nodes (22): AddressedInjectedTransaction, Announce, AnnounceData, BatchCommitment, BlockChain, BlockData, BlockFullData, BlockHeader (+14 more)

### Community 17 - "Node Environment"
Cohesion: 0.06
Nodes (15): EnvNetworkConfig, EnvRpcConfig, Node, NodeConfig, ProgramCreationInfo, ReplyInfo, TestEnv, TestEnvConfig (+7 more)

### Community 18 - "Database Iterator"
Cohesion: 0.1
Nodes (18): DatabaseIterator, DatabaseIterator<S>, DatabaseIteratorError, DatabaseIteratorStorage, Node, node_hash(), setup_db(), walk_announce_outcome() (+10 more)

### Community 19 - "Default Processing Pipeline"
Cohesion: 0.07
Nodes (4): DefaultProcessing, ValidatorContext, ValidatorService, ValidatorState

### Community 20 - "Batch Processing"
Cohesion: 0.07
Nodes (23): Batch, BatchGenerator, BatchPool, BatchPool<Rng>, BatchWithSeed, blocks_window(), create_program_batch_via_multicall(), Event (+15 more)

### Community 21 - "Mirror Event Builders"
Cohesion: 0.04
Nodes (14): ExecutableBalanceTopUpRequestedEventBuilder<'a>, MessageCallFailedEventBuilder<'a>, MessageEventBuilder<'a>, MessageQueueingRequestedEventBuilder<'a>, OwnedBalanceTopUpRequestedEventBuilder<'a>, ReplyCallFailedEventBuilder<'a>, ReplyEventBuilder<'a>, ReplyQueueingRequestedEventBuilder<'a> (+6 more)

### Community 22 - "Mirror Contract Integration"
Cohesion: 0.07
Nodes (3): Mirror, MirrorEvents<'a>, MirrorQuery

### Community 23 - "Router Wrapper"
Cohesion: 0.05
Nodes (1): Router<'a>

### Community 24 - "Node Loader CLI"
Cohesion: 0.07
Nodes (22): FuzzParams, LoadParams, Params, SeedVariant, load_node(), main(), ActorStateHashWithQueueSize, chunk_partitioning() (+14 more)

### Community 25 - "Fast Sync"
Cohesion: 0.09
Nodes (19): collect_announce(), collect_code_ids(), collect_program_code_ids(), collect_program_states(), EventData, instrument_codes(), net_fetch(), RequestManager (+11 more)

### Community 26 - "In-Memory Database"
Cohesion: 0.08
Nodes (13): MemDb, create_empty(), create_with_many_pending_events(), create_with_multiple_announces(), create_with_validation_requests(), earlier_received_announces(), process_computed_block_with_unexpected_hash(), process_external_event_with_invalid_announce() (+5 more)

### Community 27 - "Peer Scoring"
Cohesion: 0.1
Nodes (12): Behaviour, Config, decay_math(), Event, Handle, Metrics, new_swarm(), new_swarm_with_config() (+4 more)

### Community 28 - "Consensus Context"
Cohesion: 0.09
Nodes (11): Context, ContextUpdate, HostContext, header(), validator_list_advances(), ValidatorList, ValidatorListSnapshot, validators_vec() (+3 more)

### Community 29 - "Announce Primitives"
Cohesion: 0.08
Nodes (14): Announce, AnnounceV2, BlockData, BlockHeader, CodeAndId, CodeAndIdUnchecked, CodeBlobInfo, panic_on_era_from_ts_before_genesis() (+6 more)

### Community 30 - "Mirror Wrapper"
Cohesion: 0.07
Nodes (1): Mirror<'a>

### Community 31 - "DB Integrity Verifier"
Cohesion: 0.1
Nodes (16): IntegrityVerifier, IntegrityVerifierError, test_block_meta_not_prepared_error(), test_block_meta_not_synced_error(), test_block_schedule_has_expired_tasks_error(), test_code_is_not_valid_error(), test_database_visitor_error_propagation(), test_invalid_block_parent_height_error() (+8 more)

### Community 32 - "SDK Instance & Errors"
Cohesion: 0.12
Nodes (14): Error, VaraEthInstance, fails_announce_missing(), fails_chain_len_exceeding_max(), fails_reaching_max_chain_length(), fails_reaching_start_non_genesis(), fails_when_reaching_genesis(), make_announce() (+6 more)

### Community 33 - "Gossip Topic Management"
Cohesion: 0.19
Nodes (22): current_era_address_is_not_validator(), Metrics, new_era(), new_era_address_is_not_validator(), new_snapshot(), new_topic(), new_validator_message(), next_era_address_is_not_validator() (+14 more)

### Community 34 - "RPC Program API"
Cohesion: 0.09
Nodes (7): FullProgramState, Program, ProgramApi, EthexeHostLazyPages, PageKey, ThreadParams, with_params()

### Community 35 - "CLI Configuration"
Cohesion: 0.1
Nodes (6): Config, ConfigPublicKey, NodeConfig, NodeParams, VaraEth, wait_for_rpc()

### Community 36 - "Ethereum Provider"
Cohesion: 0.11
Nodes (6): BlockId, create_provider(), Ethereum, PendingTransactionBuilder<network::Ethereum>, Sender, sender_signs_prehashed_message()

### Community 37 - "Compute Service"
Cohesion: 0.14
Nodes (17): AnnouncePromisesStream, block_events(), canonical_event(), collect_not_computed_predecessors(), collect_not_computed_predecessors_work_correctly(), ComputeConfig, ComputeSubService, ComputeSubService<P> (+9 more)

### Community 38 - "Hash Primitives"
Cohesion: 0.1
Nodes (3): HashOf<T>, MaybeHashOf<T>, option_string()

### Community 39 - "WVara Token Contract"
Cohesion: 0.11
Nodes (3): WVara, WVaraEvents<'a>, WVaraQuery

### Community 40 - "State Transitions"
Cohesion: 0.09
Nodes (3): FinalizedBlockTransitions, InBlockTransitions, NonFinalTransition

### Community 41 - "Consensus Initial State"
Cohesion: 0.21
Nodes (14): announce_propagation_done(), announce_propagation_many_missing_blocks(), commitment_with_delay(), create_initial_success(), create_with_chain_head_success(), Initial, missing_announces_request_response(), process_announces_response_rejected() (+6 more)

### Community 42 - "Announce Storage"
Cohesion: 0.08
Nodes (22): AnnounceMeta, AnnounceStorageRO, AnnounceStorageRW, BlockMeta, BlockMetaStorageRO, BlockMetaStorageRW, CodesStorageRO, CodesStorageRW (+14 more)

### Community 43 - "Journal Handler"
Cohesion: 0.1
Nodes (1): NativeJournalHandler<'_, S>

### Community 44 - "RocksDB Backend"
Cohesion: 0.14
Nodes (10): cas_multi_thread(), cas_read_write(), configure_rocksdb(), is_cloneable(), kv_iter_prefix(), kv_multi_thread(), kv_read_write(), PrefixIterator (+2 more)

### Community 45 - "Producer State Machine"
Cohesion: 0.2
Nodes (10): code_commitments_only(), create(), Producer, ProducerExt, simple(), State, threshold_one(), threshold_two() (+2 more)

### Community 46 - "Transaction Commands"
Cohesion: 0.13
Nodes (15): ClaimValueResult, CreateResultData, explorer_address_link(), explorer_base(), explorer_link(), MirrorState, SendMessagePayload, SendMessageResult (+7 more)

### Community 47 - "RPC Code API"
Cohesion: 0.12
Nodes (8): Code, CodeApi, CodesSubService, CodesSubService<P>, Metrics, process_already_validated_code(), process_code(), process_invalid_code()

### Community 48 - "Contract Deployment"
Cohesion: 0.19
Nodes (9): aggregated_public_key(), ContractsDeploymentParams, deploy_middleware(), deploy_router(), deploy_wrapped_vara(), EthereumDeployer, generate_secret_sharing_commitment(), SymbioticOperatorConfig (+1 more)

### Community 49 - "Gossipsub Protocol"
Cohesion: 0.14
Nodes (4): Behaviour, Event, Message, MessageValidator

### Community 50 - "Block Preparation"
Cohesion: 0.2
Nodes (13): collect_not_prepared_blocks_chain(), ComputeEvent, Event, Metrics, missing_data(), MissingData, prepare_one_block(), PrepareSubService (+5 more)

### Community 51 - "Connect Service"
Cohesion: 0.16
Nodes (2): announce_not_computed_after_pending_and_rejected(), ConnectService

### Community 52 - "WASM Instance Creator"
Cohesion: 0.24
Nodes (2): InstanceCreator, InstanceWrapper

### Community 53 - "CAS Overlay"
Cohesion: 0.18
Nodes (2): CASOverlay, KVOverlay

### Community 54 - "Participant State Machine"
Cohesion: 0.24
Nodes (10): codes_not_waiting_for_commitment_error(), create(), create_with_pending_events(), digest_mismatch_warning(), duplicate_codes_warning(), empty_batch_error(), Participant, process_validation_request_failure() (+2 more)

### Community 55 - "Runtime Interface Extension"
Cohesion: 0.12
Nodes (1): Ext<RI>

### Community 56 - "Execution Journal"
Cohesion: 0.18
Nodes (8): charge_exec_balance(), init_setup(), Limiter, LimitsStatus, NativeJournalHandler, notes_update_state_hash(), RuntimeJournalHandler, RuntimeJournalHandler<'_, S>

### Community 57 - "WASM Sandbox"
Cohesion: 0.13
Nodes (0): 

### Community 58 - "Documentation"
Cohesion: 0.14
Nodes (15): Anvil Mnemonic (account derivation), Ethexe (VARA-ETH), ethexe-cli (referenced), ethexe-common crate, ethexe-node-loader, ethexe-rpc crate, gprimitives crate, gsigner crate (+7 more)

### Community 59 - "Coordinator State Machine"
Cohesion: 0.26
Nodes (3): Coordinator, coordinator_create_success(), process_validation_reply()

### Community 60 - "Validator Set"
Cohesion: 0.18
Nodes (3): EmptyValidatorsError, ValidatorsVec, Vec<gear_core::ids::ActorId>

### Community 61 - "WASM Runtime"
Cohesion: 0.33
Nodes (1): Runtime

### Community 62 - "Lazy Pages"
Cohesion: 0.22
Nodes (0): 

### Community 63 - "DB Check Command"
Cohesion: 0.33
Nodes (3): announce_block(), CheckCommand, Checker

### Community 64 - "Timer Utility"
Cohesion: 0.28
Nodes (1): Timer<T>

### Community 65 - "Batch Validation Messages"
Cohesion: 0.25
Nodes (2): BatchCommitmentValidationReply, BatchCommitmentValidationRequest

### Community 66 - "Run Reports"
Cohesion: 0.29
Nodes (3): BatchRunReport, MailboxReport, Report

### Community 67 - "Thread Pool"
Cohesion: 0.38
Nodes (3): test_thread_pool(), ThreadPool, ThreadPool<I, O>

### Community 68 - "WASM Allocator"
Cohesion: 0.38
Nodes (3): free(), malloc(), RuntimeAllocator

### Community 69 - "Metrics"
Cohesion: 0.38
Nodes (2): InjectedApiMetrics, Libp2pMetrics

### Community 70 - "Schedule Handler"
Cohesion: 0.29
Nodes (1): Handler<'_, S>

### Community 71 - "RPC Parameters"
Cohesion: 0.29
Nodes (2): Cors, RpcParams

### Community 72 - "Clones Codegen"
Cohesion: 0.38
Nodes (6): BytecodeContent, Cli, generate_to_file(), main(), replace_placeholder_with_zeros(), SolidityBuildArtifact

### Community 73 - "Memory Wrapper"
Cohesion: 0.6
Nodes (1): MemoryWrap

### Community 74 - "Key Management"
Cohesion: 0.47
Nodes (3): apply_default_storage(), apply_default_storage_keyring(), KeyCommand

### Community 75 - "Testing Event Receiver"
Cohesion: 0.53
Nodes (1): TestingEventReceiver

### Community 76 - "Batch Generator RNG"
Cohesion: 0.5
Nodes (1): BatchGenerator<Rng>

### Community 77 - "Fuzz Command Gen"
Cohesion: 0.83
Nodes (3): generate_fuzz_commands(), generate_one(), random_bytes()

### Community 78 - "Event Processing Handler"
Cohesion: 0.5
Nodes (1): ProcessingHandler

### Community 79 - "Format Utilities"
Cohesion: 0.5
Nodes (1): RawOrFormattedValue<C>

### Community 80 - "DB Initialization"
Cohesion: 0.83
Nodes (3): initialize_db(), initialize_empty_db(), validate_db()

### Community 81 - "Task Local Storage"
Cohesion: 0.67
Nodes (1): LocalKey<T>

### Community 82 - "Event Receiver"
Cohesion: 0.5
Nodes (1): EventReceiver<T>

### Community 83 - "Observer Event Receiver"
Cohesion: 0.67
Nodes (1): ObserverEventReceiver

### Community 84 - "Mock Signer"
Cohesion: 0.67
Nodes (1): Signer

### Community 85 - "Mock Validator State"
Cohesion: 0.67
Nodes (1): ValidatorState

### Community 86 - "Build Script"
Cohesion: 1.0
Nodes (2): main(), skip_build_on_intellij_sync()

### Community 87 - "Mock Database"
Cohesion: 0.67
Nodes (1): DB

### Community 88 - "Migration V1"
Cohesion: 0.67
Nodes (0): 

### Community 89 - "Migration V0"
Cohesion: 0.67
Nodes (2): LatestData, ProtocolTimelines

### Community 90 - "Migration V2"
Cohesion: 0.67
Nodes (0): 

### Community 91 - "FuturesUnordered Helper"
Cohesion: 0.67
Nodes (1): &mut FuturesUnordered<F>

### Community 92 - "BatchCommitter Trait Object"
Cohesion: 1.0
Nodes (1): Box<dyn BatchCommitter>

### Community 93 - "Validation Reject Reason"
Cohesion: 1.0
Nodes (1): ValidationRejectReason

### Community 94 - "Context Update Report"
Cohesion: 1.0
Nodes (1): ContextUpdate

### Community 95 - "Batch Seed"
Cohesion: 1.0
Nodes (1): (Seed, Batch)

### Community 96 - "Collection Format Utils"
Cohesion: 1.0
Nodes (1): AlternateCollectionFmt<&'a BTreeSet<T>>

### Community 97 - "Transition Controller"
Cohesion: 1.0
Nodes (1): TransitionController<'_, S>

### Community 98 - "Expiring State"
Cohesion: 1.0
Nodes (1): Expiring<T>

### Community 99 - "RPC Option Conversion"
Cohesion: 1.0
Nodes (1): Option<Vec<String>>

### Community 100 - "Validator Address Conversion"
Cohesion: 1.0
Nodes (1): Vec<Address>

### Community 101 - "Mock Protocol Timelines"
Cohesion: 1.0
Nodes (1): ProtocolTimelines

### Community 102 - "Mock Code Commitment"
Cohesion: 1.0
Nodes (1): CodeCommitment

### Community 103 - "Mock Batch Validation"
Cohesion: 1.0
Nodes (1): BatchCommitmentValidationRequest

### Community 104 - "Mock State Transition"
Cohesion: 1.0
Nodes (1): StateTransition

### Community 105 - "Mock Injected Transaction"
Cohesion: 1.0
Nodes (1): InjectedTransaction

### Community 106 - "Validator Message Hashing"
Cohesion: 1.0
Nodes (1): ValidatorMessage<T>

### Community 107 - "Migration Function"
Cohesion: 1.0
Nodes (1): F

### Community 108 - "Task Local Key"
Cohesion: 1.0
Nodes (1): LocalKey

### Community 109 - "Event Sender"
Cohesion: 1.0
Nodes (1): EventSender<T>

### Community 110 - "H256 Block ID"
Cohesion: 1.0
Nodes (1): H256

### Community 111 - "U32 Block ID"
Cohesion: 1.0
Nodes (1): u32

### Community 112 - "U64 Block ID"
Cohesion: 1.0
Nodes (1): u64

### Community 113 - "Aggregated Public Key"
Cohesion: 1.0
Nodes (1): Gear::AggregatedPublicKey

### Community 114 - "Chain Commitment"
Cohesion: 1.0
Nodes (1): Gear::ChainCommitment

### Community 115 - "Code Commitment"
Cohesion: 1.0
Nodes (1): Gear::CodeCommitment

### Community 116 - "Operator Rewards"
Cohesion: 1.0
Nodes (1): Gear::OperatorRewardsCommitment

### Community 117 - "Staker Rewards"
Cohesion: 1.0
Nodes (1): Gear::StakerRewards

### Community 118 - "Staker Rewards Commitment"
Cohesion: 1.0
Nodes (1): Gear::StakerRewardsCommitment

### Community 119 - "Rewards Commitment"
Cohesion: 1.0
Nodes (1): Gear::RewardsCommitment

### Community 120 - "Batch Commitment"
Cohesion: 1.0
Nodes (1): Gear::BatchCommitment

### Community 121 - "Gear Message"
Cohesion: 1.0
Nodes (1): Gear::Message

### Community 122 - "Reply Details"
Cohesion: 1.0
Nodes (1): Gear::ReplyDetails

### Community 123 - "State Transition"
Cohesion: 1.0
Nodes (1): Gear::StateTransition

### Community 124 - "Value Claim"
Cohesion: 1.0
Nodes (1): Gear::ValueClaim

### Community 125 - "Generic Type T"
Cohesion: 1.0
Nodes (1): T

### Community 126 - "Iterator Type T"
Cohesion: 1.0
Nodes (1): T

### Community 127 - "Option Type T"
Cohesion: 1.0
Nodes (1): Option<T>

### Community 128 - "Mut Option Type T"
Cohesion: 1.0
Nodes (1): &mut Option<T>

### Community 129 - "Events Type T"
Cohesion: 1.0
Nodes (1): T

## Knowledge Gaps
- **360 isolated node(s):** `DBAnnouncesExt`, `AnnounceRejectionReason`, `AnnounceStatus`, `PropBaseParams`, `ConsensusService` (+355 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **Thin community `BatchCommitter Trait Object`** (2 nodes): `Box<dyn BatchCommitter>`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Validation Reject Reason`** (2 nodes): `ValidationRejectReason`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Context Update Report`** (2 nodes): `ContextUpdate`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Batch Seed`** (2 nodes): `(Seed, Batch)`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Collection Format Utils`** (2 nodes): `AlternateCollectionFmt<&'a BTreeSet<T>>`, `.set()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Transition Controller`** (2 nodes): `TransitionController<'_, S>`, `.update_state()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Expiring State`** (2 nodes): `Expiring<T>`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `RPC Option Conversion`** (2 nodes): `Option<Vec<String>>`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Validator Address Conversion`** (2 nodes): `Vec<Address>`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mock Protocol Timelines`** (2 nodes): `ProtocolTimelines`, `.mock()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mock Code Commitment`** (2 nodes): `CodeCommitment`, `.mock()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mock Batch Validation`** (2 nodes): `BatchCommitmentValidationRequest`, `.mock()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mock State Transition`** (2 nodes): `StateTransition`, `.mock()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mock Injected Transaction`** (2 nodes): `InjectedTransaction`, `.mock()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Validator Message Hashing`** (2 nodes): `ValidatorMessage<T>`, `.update_hasher()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Migration Function`** (2 nodes): `F`, `.migrate()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Task Local Key`** (2 nodes): `task_local.rs`, `LocalKey`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Event Sender`** (2 nodes): `EventSender<T>`, `.send()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `H256 Block ID`** (2 nodes): `H256`, `.into_block_id()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `U32 Block ID`** (2 nodes): `u32`, `.into_block_id()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `U64 Block ID`** (2 nodes): `u64`, `.into_block_id()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Aggregated Public Key`** (2 nodes): `Gear::AggregatedPublicKey`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Chain Commitment`** (2 nodes): `Gear::ChainCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Code Commitment`** (2 nodes): `Gear::CodeCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Operator Rewards`** (2 nodes): `Gear::OperatorRewardsCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Staker Rewards`** (2 nodes): `Gear::StakerRewards`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Staker Rewards Commitment`** (2 nodes): `Gear::StakerRewardsCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Rewards Commitment`** (2 nodes): `Gear::RewardsCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Batch Commitment`** (2 nodes): `Gear::BatchCommitment`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Gear Message`** (2 nodes): `Gear::Message`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Reply Details`** (2 nodes): `Gear::ReplyDetails`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `State Transition`** (2 nodes): `Gear::StateTransition`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Value Claim`** (2 nodes): `Gear::ValueClaim`, `.from()`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Generic Type T`** (1 nodes): `T`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Iterator Type T`** (1 nodes): `T`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Option Type T`** (1 nodes): `Option<T>`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Mut Option Type T`** (1 nodes): `&mut Option<T>`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.
- **Thin community `Events Type T`** (1 nodes): `T`
  Too small to be a meaningful cluster - may be noise or needs more connections extracted.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What connects `DBAnnouncesExt`, `AnnounceRejectionReason`, `AnnounceStatus` to the rest of the system?**
  _360 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Service Orchestration` be split into smaller, more focused modules?**
  _Cohesion score 0.02 - nodes in this community are weakly interconnected._
- **Should `Runtime Common & State Migration` be split into smaller, more focused modules?**
  _Cohesion score 0.02 - nodes in this community are weakly interconnected._
- **Should `RPC Block API & Database` be split into smaller, more focused modules?**
  _Cohesion score 0.02 - nodes in this community are weakly interconnected._
- **Should `Runtime Logging` be split into smaller, more focused modules?**
  _Cohesion score 0.02 - nodes in this community are weakly interconnected._
- **Should `Program State Management` be split into smaller, more focused modules?**
  _Cohesion score 0.03 - nodes in this community are weakly interconnected._
- **Should `Ethereum SDK & API` be split into smaller, more focused modules?**
  _Cohesion score 0.02 - nodes in this community are weakly interconnected._