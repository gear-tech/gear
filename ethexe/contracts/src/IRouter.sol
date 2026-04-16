// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {Gear} from "./libraries/Gear.sol";

/**
 * @dev Interface for the `Router` contract.
 *      The Router serves as the primary entry point representing a co-processor instance.
 *      It emits two types of events: *informational* events, which are intended to notify external users
 *      of actions that have occurred within the co-processor, and *requesting* events, which are intended
 *      to request processing logic from validator nodes.
 * @notice The Router interface provides basic co-processor functionalities, such as WASM submission,
 *         program creation, and result settlement, acting as an authority for acknowledged programs,
 *         driven by validator signature verification.
 */
interface IRouter {
    /* # Structs */

    /**
     * @dev Represents view of the `Router` storage.
     */
    struct StorageView {
        /**
         * @notice Genesis block information for this router.
         */
        Gear.GenesisBlockInfo genesisBlock;
        /**
         * @notice Information about the latest committed batch.
         */
        Gear.CommittedBatchInfo latestCommittedBatch;
        /**
         * @notice Details of the related contracts' implementation.
         */
        Gear.AddressBook implAddresses;
        /**
         * @notice Parameters for validation and signature verification.
         */
        Gear.ValidationSettingsView validationSettings;
        /**
         * @notice Computation parameters for programs processing.
         */
        Gear.ComputationSettings computeSettings;
        /**
         * @notice Protocol timelines.
         */
        Gear.Timelines timelines;
        /**
         * @notice Count of created programs.
         */
        uint256 programsCount;
        /**
         * @notice Count of validated codes.
         */
        uint256 validatedCodesCount;
    }

    /**
     * @custom:storage-location erc7201:router.storage.RouterV1
     */
    struct Storage {
        /**
         * @notice Reserved storage slot.
         * @dev This slot is reserved for gas estimation purposes. Must be zero.
         *      See `ethexe/ethereum/src/router/mod.rs` for details about gas estimation.
         */
        uint256 reserved;
        /**
         * @notice Genesis block information for this router.
         * @dev This identifies the co-processor instance.
         *      To allow interactions with the router, after initialization,
         *      someone must call `lookupGenesisHash()`.
         */
        Gear.GenesisBlockInfo genesisBlock;
        /**
         * @notice Information about the latest committed batch.
         */
        Gear.CommittedBatchInfo latestCommittedBatch;
        /**
         * @notice Details of the related contracts' implementation.
         */
        Gear.AddressBook implAddresses;
        /**
         * @notice Parameters for validation and signature verification.
         * @dev This contains information about the validator set and
         *      the verification threshold for signatures.
         */
        Gear.ValidationSettings validationSettings;
        /**
         * @notice Computation parameters for programs processing.
         * @dev These parameters should be used for the operational logic of
         *      event and message handling on nodes. Any modifications will take
         *      effect in the next block.
         */
        Gear.ComputationSettings computeSettings;
        /**
         * @notice Protocol timelines.
         * @dev This contains information about the protocol's timelines.
         */
        Gear.Timelines timelines;
        /**
         * @notice Gear protocol data related to this router instance.
         * @dev This contains information about the available codes and programs.
         */
        Gear.ProtocolData protocolData;
    }

    /* # Events */

    /**
     * @notice Emitted when batch of commitments has been applied.
     * @dev This is an *informational* event, signaling that all commitments in batch has been applied.
     * @param hash Batch hash (`keccak256` algorithm), see `Gear.batchCommitmentHash(...)`.
     */
    event BatchCommitted(bytes32 hash);

    /**
     * @notice Emitted when all necessary state transitions have been applied and states have changed.
     * @dev This is an *informational* event, signaling that the all transitions until head were committed.
     * @param head The hash of committed announces chain head.
     */
    event AnnouncesCommitted(bytes32 head);

    /**
     * @notice Emitted when a code, previously requested for validation, receives validation results, so its `Gear.CodeState` changed.
     * @dev This is an *informational* event, signaling the results of code validation.
     * @param codeId The ID of the code that was validated. It's calculated as `gprimitives::CodeId::generate(wasm_code)` (blake2b hash).
     * @param valid The result of the validation: indicates whether the code ID can be used for program creation.
     */
    event CodeGotValidated(bytes32 codeId, bool indexed valid);

    /**
     * @notice Emitted when a new code validation request is submitted.
     * @dev This is a *requesting* event, signaling that validators need to download and validate the code from the transaction blob.
     * @param codeId The expected code ID of the applied WASM blob, one the client side it's calculated as `gprimitives::CodeId::generate(wasm_code)`.
     */
    event CodeValidationRequested(bytes32 codeId);

    /**
     * @notice Emitted when validators for the next era has been set.
     * @dev This is an *informational* and *request* event, signaling that validators has been set for the next era.
     * @param eraIndex The index of the era for which the validators have been committed.
     */
    event ValidatorsCommittedForEra(uint256 eraIndex);

    /**
     * @notice Emitted when the computation settings have been changed.
     * @dev This is both an *informational* and *requesting* event, signaling that an authority decided to change the computation settings.
     *      Users and program authors may want to adjust their practices, while validators need to apply the changes internally starting from the next block.
     * @param threshold The amount of Gear gas initially allocated for free to allow the program to decide if it wants to process the incoming message.
     * @param wvaraPerSecond The amount of WVara to be charged from the program's execution balance per second of computation.
     */
    event ComputationSettingsChanged(uint64 threshold, uint128 wvaraPerSecond);

    /**
     * @notice Emitted when a new program within the co-processor is created and is now available on-chain.
     * @dev This is both an *informational* and *requesting* event, signaling the creation of a new program and its Ethereum mirror.
     *      Validators need to initialize it with a zeroed hash state internally.
     * @param actorId ID of the actor that was created. It is accessible inside the co-processor and on Ethereum by this identifier.
     * @param codeId The code ID of the WASM implementation of the created program. It's calculated as `gprimitives::CodeId::generate(wasm_code)` (blake2b hash).
     */
    event ProgramCreated(address actorId, bytes32 indexed codeId);

    /**
     * @notice Emitted when the router's storage slot has been changed.
     * @dev This is both an *informational* and *requesting* event, signaling that an authority decided to wipe the router state,
     *      rendering all previously existing codes and programs ineligible. Validators need to wipe their databases immediately.
     * @param slot The new storage slot.
     */
    event StorageSlotChanged(bytes32 slot);

    /* # Errors */

    /**
     * @dev Thrown when an invalid block.timestamp is provided (must be greater than 0).
     */
    error InvalidTimestamp();

    /**
     * @dev Thrown when an invalid election duration is provided (must be greater than 0).
     */
    error InvalidElectionDuration();

    /**
     * @dev Thrown when the era duration is too short (era duration must be greater than election duration).
     */
    error EraDurationTooShort();

    error InvalidFROSTAggregatedPublicKey();

    /**
     * @dev Thrown when the validation delay is too big.
     */
    error ValidationDelayTooBig();

    /**
     * @dev Thrown when the genesis hash is already set by someone else.
     */
    error GenesisHashAlreadySet();

    /**
     * @dev Thrown when the genesis hash is not found from previous blocks.
     *      There is 256 blocks lookback for `blockhash` opcode,
     *      so if the genesis block is too old, it may not be found.
     */
    error GenesisHashNotFound();

    /**
     * @dev Thrown when the tx is not in EIP-7594 format,
     *      so it doesn't have blobhashes.
     */
    error BlobNotFound();

    /**
     * @dev Thrown when the router's genesis hash is not initialized
     *      (no one called `lookupGenesisHash` yet).
     */
    error RouterGenesisHashNotInitialized();

    /**
     * @dev Thrown when the code is already on validation or validated.
     */
    error CodeAlreadyOnValidationOrValidated();

    /**
     * @dev Thrown when the code is not validated and someone tries to create program with it.
     */
    error CodeNotValidated();

    error PredecessorBlockNotFound();

    error BatchTimestampNotInPast();

    error InvalidPreviousCommittedBatchHash();

    error BatchTimestampTooEarly();

    error TooManyChainCommitments();

    error UnknownProgram();

    error CodeValidationNotRequested();

    error TooManyRewardsCommitments();

    error RewardsCommitmentTimestampNotInPast();

    error RewardsCommitmentPredatesGenesis();

    error RewardsCommitmentEraNotPrevious();

    error ApproveERC20Failed();

    error TooManyValidatorsCommitments();

    error EmptyValidatorsList();

    error CommitmentEraNotNext();

    error ElectionNotStarted();

    error ValidatorsAlreadyScheduled();

    error SignatureVerificationFailed();

    error ZeroValueTransfer();

    /* # Views */

    /**
     * @dev Returns the storage view of the contract storage.
     * @return storageView The storage view of the contract storage.
     */
    function storageView() external view returns (StorageView memory);

    /**
     * @dev Returns the hash of the genesis block.
     * @return genesisBlockHash The hash of the genesis block.
     */
    function genesisBlockHash() external view returns (bytes32);

    /**
     * @dev Returns the timestamp of the genesis block.
     * @return genesisTimestamp The timestamp of the genesis block.
     */
    function genesisTimestamp() external view returns (uint48);

    /**
     * @dev Returns the hash of the latest committed batch.
     * @return latestCommittedBatchHash The hash of the latest committed batch.
     */
    function latestCommittedBatchHash() external view returns (bytes32);

    /**
     * @dev Returns the timestamp of the latest committed batch.
     * @return latestCommittedBatchTimestamp The timestamp of the latest committed batch.
     */
    function latestCommittedBatchTimestamp() external view returns (uint48);

    /**
     * @dev Returns the address of the mirror implementation.
     * @return mirrorImpl The address of the mirror implementation.
     */
    function mirrorImpl() external view returns (address);

    /**
     * @dev Returns the address of the wrapped Vara implementation.
     * @return wrappedVara The address of the wrapped Vara implementation.
     */
    function wrappedVara() external view returns (address);

    /**
     * @dev Returns the address of the middleware implementation.
     * @return middleware The address of the middleware implementation.
     */
    function middleware() external view returns (address);

    /**
     * @dev Returns the aggregated public key of the current validators.
     * @return validatorsAggregatedPublicKey The aggregated public key of the current validators.
     */
    function validatorsAggregatedPublicKey() external view returns (Gear.AggregatedPublicKey memory);

    /**
     * @dev Returns the verifiable secret sharing commitment of the current validators.
     *      This is serialized `frost_core::keys::VerifiableSecretSharingCommitment` struct.
     *      See https://docs.rs/frost-core/latest/frost_core/keys/struct.VerifiableSecretSharingCommitment.html#method.serialize_whole.
     * @return validatorsVerifiableSecretSharingCommitment The verifiable secret sharing commitment of the current validators.
     */
    function validatorsVerifiableSecretSharingCommitment() external view returns (bytes memory);

    /**
     * @dev Checks if the given addresses are all validators.
     * @return areValidators `true` if all addresses are validators, `false` otherwise.
     */
    function areValidators(address[] calldata validators) external view returns (bool);

    /**
     * @dev Checks if the given address is a validator.
     * @return isValidator `true` if the address is a validator, `false` otherwise.
     */
    function isValidator(address validator) external view returns (bool);

    /**
     * @dev Returns the signing threshold fraction.
     * @return thresholdNumerator The numerator of the signing threshold fraction.
     * @return thresholdDenominator The denominator of the signing threshold fraction.
     */
    function signingThresholdFraction() external view returns (uint128, uint128);

    /**
     * @dev Returns the list of current validators.
     * @return validators The list of current validators.
     */
    function validators() external view returns (address[] memory);

    /**
     * @dev Returns the count of current validators.
     * @return validatorsCount The count of current validators.
     */
    function validatorsCount() external view returns (uint256);

    /**
     * @dev Returns the threshold number of validators required for a valid signature.
     * @return threshold The threshold number of validators required for a valid signature.
     */
    function validatorsThreshold() external view returns (uint256);

    /**
     * @dev Returns true if the contract is paused, and false otherwise.
     * @return isPaused `true` if the contract is paused, `false` otherwise.
     */
    function paused() external view returns (bool);

    /**
     * @dev Returns the computation settings.
     * @return computeSettings The computation settings.
     */
    function computeSettings() external view returns (Gear.ComputationSettings memory);

    /**
     * @dev Returns the state of code.
     * @return codeState The state of the code.
     */
    function codeState(bytes32 codeId) external view returns (Gear.CodeState);

    /**
     * @dev Returns the states of multiple codes.
     * @return codesStates The states of the codes.
     */
    function codesStates(bytes32[] calldata codesIds) external view returns (Gear.CodeState[] memory);

    /**
     * @dev Returns the code ID of the given program.
     * @return codeId The code ID of the program.
     */
    function programCodeId(address programId) external view returns (bytes32);

    /**
     * @dev Returns the code IDs of the given programs.
     * @return codesIds The code IDs of the programs.
     */
    function programsCodeIds(address[] calldata programsIds) external view returns (bytes32[] memory);

    /**
     * @dev Returns the count of programs.
     * @return programsCount The count of programs.
     */
    function programsCount() external view returns (uint256);

    /**
     * @dev Returns the count of validated codes.
     * @return validatedCodesCount The count of validated codes.
     */
    function validatedCodesCount() external view returns (uint256);

    /**
     * @dev Returns the timelines.
     * @return timelines The timelines.
     */
    function timelines() external view returns (Gear.Timelines memory);

    /* # Owner calls */

    /**
     * @dev Sets the `Mirror` implementation address.
     * @param newMirror The new mirror implementation address.
     */
    function setMirror(address newMirror) external;

    /**
     * @dev Pauses the contract.
     */
    function pause() external;

    /**
     * @dev Unpauses the contract.
     */
    function unpause() external;

    /* # Calls */

    /**
     * @dev Looks up the genesis hash from previous blocks.
     */
    function lookupGenesisHash() external;

    /**
     * @dev Requests code validation for the given code ID.
     *      This method is expected to be called within EIP-7594 transaction and will have sidecar
     *      attached to it containing WASM bytecode. On EVM, we can only verify that there was
     *      at least 1 blobhash in a transaction.
     * @param codeId The expected code ID for which the validation is requested.
     *               It's calculated as `gprimitives::CodeId::generate(wasm_code)` (blake2b hash).
     */
    function requestCodeValidation(bytes32 codeId) external;

    /**
     * @dev Creates new program (`Mirror`) with the given code ID, salt, and initializer.
     *      Note that the program creation is deterministic, so if you try to create program with the same code ID and salt,
     *      you will get the same program address.
     *      Also note that the `Mirror` will be created with `isSmall = true` without "Solidity ABI Interface" support,
     *      so it will be more gas efficient, but services like Etherscan won't be able to encode some calls and decode some events.
     *      As result of execution, the `ProgramCreated` event will be emitted.
     * @param codeId The code ID of the program to create. Must be in `CodeState.Validated` state.
     * @param salt The salt for the program creation.
     * @param overrideInitializer The initializer address for the program that can send the first (init) message to the program.
     *                            If set to `address(0)`, `msg.sender` will be used as the initializer.
     * @return mirror The address of the created program (`Mirror`).
     */
    function createProgram(bytes32 codeId, bytes32 salt, address overrideInitializer) external returns (address);

    /**
     * @dev Creates new program (`Mirror`) with the given code ID, salt, initializer and ABI interface.
     *      Note that the program creation is deterministic, so if you try to create program with the same code ID and salt,
     *      you will get the same program address.
     *      Also note that the `Mirror` will be created with `isSmall = false` WITH "Solidity ABI Interface" support,
     *      so it will be less gas efficient, but services like Etherscan will be able to encode some calls and decode some events.
     *      As result of execution, the `ProgramCreated` event will be emitted.
     * @param codeId The code ID of the program to create. Must be in `CodeState.Validated` state.
     * @param salt The salt for the program creation.
     * @param overrideInitializer The initializer address for the program that can send the first (init) message to the program.
     *                            If set to `address(0)`, `msg.sender` will be used as the initializer.
     * @param abiInterface The ABI interface address for the program.
     * @return mirror The address of the created program (`Mirror`).
     */
    function createProgramWithAbiInterface(
        bytes32 codeId,
        bytes32 salt,
        address overrideInitializer,
        address abiInterface
    ) external returns (address);

    /**
     * @dev Commits new batch of changes to `Router` state.
     *      `CodeGotValidated` event is emitted for each code in commitment.
     *      `AnnouncesCommitted` event is emitted on success. Triggers multiple events for each corresponding `Mirror` instances.
     * @param batchCommitment The batch commitment data.
     * @param signatureType The type of signature to validate.
     * @param signatures The signatures for the batch commitment.
     */
    function commitBatch(
        Gear.BatchCommitment calldata batchCommitment,
        Gear.SignatureType signatureType,
        bytes[] calldata signatures
    ) external;
}
