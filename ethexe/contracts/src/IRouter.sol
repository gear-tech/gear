// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Gear} from "./libraries/Gear.sol";

/// @title Gear.exe Router Interface
/// @notice The Router interface provides basic co-processor functionalities, such as WASM submission, program creation, and result settlement, acting as an authority for acknowledged programs, driven by validator signature verification.
/// @dev The Router serves as the primary entry point representing a co-processor instance. It emits two types of events: *informational* events, which are intended to notify external users of actions that have occurred within the co-processor, and *requesting* events, which are intended to request processing logic from validator nodes.
interface IRouter {
    // # Structs.

    struct StorageView {
        /// @notice Genesis block information for this router.
        Gear.GenesisBlockInfo genesisBlock;
        /// @notice Information about the latest committed batch.
        Gear.CommittedBatchInfo latestCommittedBatch;
        /// @notice Details of the related contracts' implementation.
        Gear.AddressBook implAddresses;
        /// @notice Parameters for validation and signature verification.
        Gear.ValidationSettingsView validationSettings;
        /// @notice Computation parameters for programs processing.
        Gear.ComputationSettings computeSettings;
        /// @notice Protocol timelines.
        Gear.Timelines timelines;
        /// @notice Count of created programs.
        uint256 programsCount;
        /// @notice Count of validated codes.
        uint256 validatedCodesCount;
    }

    /// @custom:storage-location erc7201:router.storage.Router.
    struct Storage {
        /// @notice Reserved storage slot.
        /// @dev This slot is reserved for gas estimation purposes. Must be zero.
        uint256 reserved;
        /// @notice Genesis block information for this router.
        /// @dev This identifies the co-processor instance. To allow interactions with the router, after initialization, someone must call `lookupGenesisHash()`.
        Gear.GenesisBlockInfo genesisBlock;
        /// @notice Information about the latest committed batch.
        Gear.CommittedBatchInfo latestCommittedBatch;
        /// @notice Details of the related contracts' implementation.
        Gear.AddressBook implAddresses;
        /// @notice Parameters for validation and signature verification.
        /// @dev This contains information about the validator set and the verification threshold for signatures.
        Gear.ValidationSettings validationSettings;
        /// @notice Computation parameters for programs processing.
        /// @dev These parameters should be used for the operational logic of event and message handling on nodes. Any modifications will take effect in the next block.
        Gear.ComputationSettings computeSettings;
        /// @notice Protocol timelines.
        /// @dev This contains information about the protocol's timelines.
        Gear.Timelines timelines;
        /// @notice Gear protocol data related to this router instance.
        /// @dev This contains information about the available codes and programs.
        Gear.ProtocolData protocolData;
    }

    // # Events.

    /// @notice Emitted when batch of commitments has been applied.
    /// @dev This is an *informational* event, signaling that all commitments in batch has been applied.
    /// @param hash Batch keccak256 hash, see Gear.batchCommitmentHash.
    event BatchCommitted(bytes32 hash);

    /// @notice Emitted when all necessary state transitions have been applied and states have changed.
    /// @dev This is an *informational* event, signaling that the all transitions until head were committed.
    /// @param head The hash of committed announces chain head.
    event AnnouncesCommitted(bytes32 head);

    /// @notice Emitted when a code, previously requested for validation, receives validation results, so its CodeStatus changed.
    /// @dev This is an *informational* event, signaling the results of code validation.
    /// @param codeId The ID of the code that was validated.
    /// @param valid The result of the validation: indicates whether the code ID can be used for program creation.
    event CodeGotValidated(bytes32 codeId, bool indexed valid);

    /// @notice Emitted when a new code validation request is submitted.
    /// @dev This is a *requesting* event, signaling that validators need to download and validate the code from the transaction blob.
    /// @param codeId The expected code ID of the applied WASM blob, represented as a Blake2 hash.
    event CodeValidationRequested(bytes32 codeId);

    /// @notice Emitted when validators for the next era has been set.
    /// @dev This is an *informational* and *request* event, signaling that validators has been set for the next era.
    /// @param eraIndex The index of the era for which the validators have been committed.
    event ValidatorsCommittedForEra(uint256 eraIndex);

    /// @notice Emitted when the computation settings have been changed.
    /// @dev This is both an *informational* and *requesting* event, signaling that an authority decided to change the computation settings. Users and program authors may want to adjust their practices, while validators need to apply the changes internally starting from the next block.
    /// @param threshold The amount of Gear gas initially allocated for free to allow the program to decide if it wants to process the incoming message.
    /// @param wvaraPerSecond The amount of WVara to be charged from the program's execution balance per second of computation.
    event ComputationSettingsChanged(uint64 threshold, uint128 wvaraPerSecond);

    /// @notice Emitted when a new program within the co-processor is created and is now available on-chain.
    /// @dev This is both an *informational* and *requesting* event, signaling the creation of a new program and its Ethereum mirror. Validators need to initialize it with a zeroed hash state internally.
    /// @param actorId ID of the actor that was created. It is accessible inside the co-processor and on Ethereum by this identifier.
    /// @param codeId The code ID of the WASM implementation of the created program.
    event ProgramCreated(address actorId, bytes32 indexed codeId);

    /// @notice Emitted when the router's storage slot has been changed.
    /// @dev This is both an *informational* and *requesting* event, signaling that an authority decided to wipe the router state, rendering all previously existing codes and programs ineligible. Validators need to wipe their databases immediately.
    event StorageSlotChanged(bytes32 slot);

    // # Errors.

    error InvalidTimestamp();

    error InvalidElectionDuration();

    error EraDurationTooShort();

    error ValidationDelayTooBig();

    error GenesisHashAlreadySet();

    error GenesisHashNotFound();

    error BlobNotFound();

    error RouterGenesisHashNotInitialized();

    error CodeAlreadyOnValidationOrValidated();

    error PredecessorBlockNotFound();

    error BatchTimestampNotInPast();

    error InvalidPreviousCommittedBatchHash();

    error BatchTimestampTooEarly();

    error SignatureVerificationFailed();

    error CodeNotValidated();

    error TooManyChainCommitments();

    error CodeValidationNotRequested();

    error TooManyRewardsCommitments();

    error RewardsCommitmentTimestampNotInPast();

    error RewardsCommitmentPredatesGenesis();

    error RewardsCommitmentEraNotPrevious();

    error TooManyValidatorsCommitments();

    error EmptyValidatorsList();

    error CommitmentEraNotNext();

    error ElectionNotStarted();

    error ValidatorsAlreadyScheduled();

    error UnknownProgram();

    error InvalidFROSTAggregatedPublicKey();

    error ZeroValueTransfer();

    // # Views.
    function genesisBlockHash() external view returns (bytes32);
    function genesisTimestamp() external view returns (uint48);
    function latestCommittedBatchHash() external view returns (bytes32);
    function latestCommittedBatchTimestamp() external view returns (uint48);

    function mirrorImpl() external view returns (address);
    function wrappedVara() external view returns (address);
    function middleware() external view returns (address);

    function validatorsAggregatedPublicKey() external view returns (Gear.AggregatedPublicKey memory);
    function validatorsVerifiableSecretSharingCommitment() external view returns (bytes memory);

    function areValidators(address[] calldata validators) external view returns (bool);
    function isValidator(address validator) external view returns (bool);
    function signingThresholdFraction() external view returns (uint128, uint128);
    function validators() external view returns (address[] memory);
    function validatorsCount() external view returns (uint256);
    function validatorsThreshold() external view returns (uint256);

    function computeSettings() external view returns (Gear.ComputationSettings memory);

    function codeState(bytes32 codeId) external view returns (Gear.CodeState);
    function codesStates(bytes32[] calldata codesIds) external view returns (Gear.CodeState[] memory);
    function programCodeId(address programId) external view returns (bytes32);
    function programsCodeIds(address[] calldata programsIds) external view returns (bytes32[] memory);
    function programsCount() external view returns (uint256);
    function validatedCodesCount() external view returns (uint256);

    function timelines() external view returns (Gear.Timelines memory);

    // # Owner calls.
    function setMirror(address newMirror) external;

    // # Calls.
    function lookupGenesisHash() external;

    /// @dev CodeValidationRequested Emitted on success.
    function requestCodeValidation(bytes32 codeId) external;
    /// @dev ProgramCreated Emitted on success.
    function createProgram(bytes32 codeId, bytes32 salt, address overrideInitializer) external returns (address);
    /// @dev ProgramCreated Emitted on success.
    function createProgramWithAbiInterface(
        bytes32 codeId,
        bytes32 salt,
        address overrideInitializer,
        address abiInterface
    ) external returns (address);

    /// @dev CodeGotValidated Emitted for each code in commitment.
    /// @dev BlockCommitted Emitted on success. Triggers multiple events for each corresponding mirror.
    function commitBatch(
        Gear.BatchCommitment calldata batchCommitment,
        Gear.SignatureType signatureType,
        bytes[] calldata signatures
    ) external;
}
