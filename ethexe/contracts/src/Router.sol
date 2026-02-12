// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {IMiddleware} from "./IMiddleware.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {Clones} from "./libraries/Clones.sol";
import {ClonesSmall} from "./libraries/ClonesSmall.sol";
import {Gear} from "./libraries/Gear.sol";
import {SSTORE2} from "./libraries/SSTORE2.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {
    ReentrancyGuardTransientUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SlotDerivation} from "@openzeppelin/contracts/utils/SlotDerivation.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {FROST} from "frost-secp256k1-evm/FROST.sol";
import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";

contract Router is IRouter, OwnableUpgradeable, ReentrancyGuardTransientUpgradeable {
    // keccak256(abi.encode(uint256(keccak256("router.storage.Slot")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant SLOT_STORAGE = 0x5c09ca1b9b8127a4fd9f3c384aac59b661441e820e17733753ff5f2e86e1e000;
    // keccak256(abi.encode(uint256(keccak256("router.storage.Transient")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant TRANSIENT_STORAGE = 0xf02b465737fa6045c2ff53fb2df43c66916ac2166fa303264668fb2f6a1d8c00;

    uint256 internal constant COMMIT_BATCH_BEFORE_LOADING_STORAGE = 0;
    uint256 internal constant COMMIT_BATCH_AFTER_LOADING_STORAGE = 1;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_GENESIS_HASH = 2;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_GENESIS_HASH = 3;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_RESERVED = 4;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_RESERVED = 5;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_BLOCK_IS_PREDECESSOR = 6;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_BLOCK_IS_PREDECESSOR = 7;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_BATCH_TIMESTAMP = 8;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_BATCH_TIMESTAMP = 9;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_PREVIOUS_COMMITTED_BATCH_HASH = 10;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_PREVIOUS_COMMITTED_BATCH_HASH = 11;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_BATCH_TIMESTAMP_TOO_EARLY = 12;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_BATCH_TIMESTAMP_TOO_EARLY = 13;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_TOO_MANY_CHAIN_COMMITMENTS = 14;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_TOO_MANY_CHAIN_COMMITMENTS = 15;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_CHAIN_COMMITMENTS_LENGTH = 16;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_CHAIN_COMMITMENTS_LENGTH = 17;

    uint256 internal constant COMMIT_BATCH_BEFORE_TAKING_FIRST_CHAIN_COMMITMENT = 18;
    uint256 internal constant COMMIT_BATCH_AFTER_TAKING_FIRST_CHAIN_COMMITMENT = 19;

    uint256 internal constant COMMIT_BATCH_BEFORE_TRANSITIONS_ALLOCATION = 20;
    uint256 internal constant COMMIT_BATCH_AFTER_TRANSITIONS_ALLOCATION = 21;

    uint256 internal constant COMMIT_BATCH_BEFORE_TAKING_STATE_TRANSITION = 22;
    uint256 internal constant COMMIT_BATCH_AFTER_TAKING_STATE_TRANSITION = 23;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_FOR_UNKNOWN_PROGRAM = 24;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_FOR_UNKNOWN_PROGRAM = 25;

    uint256 internal constant COMMIT_BATCH_BEFORE_CALCULATING_VALUE_TO_RECEIVE = 26;
    uint256 internal constant COMMIT_BATCH_AFTER_CALCULATING_VALUE_TO_RECEIVE = 27;

    uint256 internal constant COMMIT_BATCH_BEFORE_CALLING_PERFORM_STATE_TRANSITION = 28;
    uint256 internal constant COMMIT_BATCH_AFTER_CALLING_PERFORM_STATE_TRANSITION = 29;

    uint256 internal constant COMMIT_BATCH_BEFORE_WRITING_TRANSITION_HASH = 30;
    uint256 internal constant COMMIT_BATCH_AFTER_WRITING_TRANSITION_HASH = 31;

    uint256 internal constant COMMIT_BATCH_BEFORE_INCREMENTING_OFFSET1 = 32;
    uint256 internal constant COMMIT_BATCH_AFTER_INCREMENTING_OFFSET1 = 33;

    uint256 internal constant COMMIT_BATCH_BEFORE_KECCAK256_TRANSITIONS_HASH = 34;
    uint256 internal constant COMMIT_BATCH_AFTER_KECCAK256_TRANSITIONS_HASH = 35;

    uint256 internal constant COMMIT_BATCH_BEFORE_EMITTING_ANNOUNCES_COMMITTED = 36;
    uint256 internal constant COMMIT_BATCH_AFTER_EMITTING_ANNOUNCES_COMMITTED = 37;

    uint256 internal constant COMMIT_BATCH_BEFORE_KECCAK256_CHAIN_COMMITMENT_HASH = 38;
    uint256 internal constant COMMIT_BATCH_AFTER_KECCAK256_CHAIN_COMMITMENT_HASH = 39;

    uint256 internal constant COMMIT_BATCH_BEFORE_CODE_COMMITMENTS_ALLOCATION = 40;
    uint256 internal constant COMMIT_BATCH_AFTER_CODE_COMMITMENTS_ALLOCATION = 41;

    uint256 internal constant COMMIT_BATCH_BEFORE_TAKING_CODE_COMMITMENT = 42;
    uint256 internal constant COMMIT_BATCH_AFTER_TAKING_CODE_COMMITMENT = 43;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_FOR_INVALID_CODE_VALIDATION_STATE = 44;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_FOR_INVALID_CODE_VALIDATION_STATE = 45;

    uint256 internal constant COMMIT_BATCH_BEFORE_SETTING_VALIDATED_CODE_STATE = 46;
    uint256 internal constant COMMIT_BATCH_AFTER_SETTING_VALIDATED_CODE_STATE = 47;

    uint256 internal constant COMMIT_BATCH_BEFORE_INCREMENTING_VALIDATED_CODES_COUNT = 48;
    uint256 internal constant COMMIT_BATCH_AFTER_INCREMENTING_VALIDATED_CODES_COUNT = 49;

    uint256 internal constant COMMIT_BATCH_BEFORE_DELETING_INVALID_CODE_STATE = 50;
    uint256 internal constant COMMIT_BATCH_AFTER_DELETING_INVALID_CODE_STATE = 51;

    uint256 internal constant COMMIT_BATCH_BEFORE_EMITTING_CODE_GOT_VALIDATED = 52;
    uint256 internal constant COMMIT_BATCH_AFTER_EMITTING_CODE_GOT_VALIDATED = 53;

    uint256 internal constant COMMIT_BATCH_BEFORE_KECCAK256_CODE_COMMITMENT_HASH = 54;
    uint256 internal constant COMMIT_BATCH_AFTER_KECCAK256_CODE_COMMITMENT_HASH = 55;

    uint256 internal constant COMMIT_BATCH_BEFORE_WRITING_CODE_COMMITMENT_HASH = 56;
    uint256 internal constant COMMIT_BATCH_AFTER_WRITING_CODE_COMMITMENT_HASH = 57;

    uint256 internal constant COMMIT_BATCH_BEFORE_INCREMENTING_OFFSET2 = 58;
    uint256 internal constant COMMIT_BATCH_AFTER_INCREMENTING_OFFSET2 = 59;

    uint256 internal constant COMMIT_BATCH_BEFORE_KECCAK256_CODE_COMMITMENTS_HASH = 60;
    uint256 internal constant COMMIT_BATCH_AFTER_KECCAK256_CODE_COMMITMENTS_HASH = 61;

    uint256 internal constant COMMIT_BATCH_BEFORE_NOT_BENCHMARKED_YET = 62;
    uint256 internal constant COMMIT_BATCH_AFTER_NOT_BENCHMARKED_YET = 63;

    uint256 internal constant COMMIT_BATCH_BEFORE_HASHING_BATCH = 64;
    uint256 internal constant COMMIT_BATCH_AFTER_HASHING_BATCH = 65;

    uint256 internal constant COMMIT_BATCH_BEFORE_UPDATING_LATEST_COMMITTED_BATCH_HASH = 66;
    uint256 internal constant COMMIT_BATCH_AFTER_UPDATING_LATEST_COMMITTED_BATCH_HASH = 67;

    uint256 internal constant COMMIT_BATCH_AFTER_UPDATING_LATEST_COMMITTED_BATCH_TIMESTAMP = 68;
    uint256 internal constant COMMIT_BATCH_BEFORE_UPDATING_LATEST_COMMITTED_BATCH_TIMESTAMP = 69;

    uint256 internal constant COMMIT_BATCH_BEFORE_EMITTING_BATCH_COMMITTED = 70;
    uint256 internal constant COMMIT_BATCH_AFTER_EMITTING_BATCH_COMMITTED = 71;

    uint256 internal constant COMMIT_BATCH_BEFORE_ERA_STARTED_AT = 72;

    uint256 internal constant COMMIT_BATCH_AFTER_EXITING_VERIFY_AT_FUNCTION = 107;

    event DebugEvent(uint256 indexed topic0) anonymous;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address _owner,
        address _mirror,
        address _wrappedVara,
        address _middleware,
        uint256 _eraDuration,
        uint256 _electionDuration,
        uint256 _validationDelay,
        Gear.AggregatedPublicKey calldata _aggregatedPublicKey,
        bytes calldata _verifiableSecretSharingCommitment,
        address[] calldata _validators
    ) public initializer {
        __Ownable_init(_owner);
        __ReentrancyGuardTransient_init();

        // Because of validator storages impl we have to check, that current timestamp is greater than 0.
        require(block.timestamp > 0, InvalidTimestamp());
        require(_electionDuration > 0, InvalidElectionDuration());
        require(_eraDuration > _electionDuration, EraDurationTooShort());
        // _validationDelay must be small enough,
        // in order to restrict old era validators to make commitments, which can damage the system.
        require(_validationDelay < (_eraDuration - _electionDuration) / 10, ValidationDelayTooBig());

        _setStorageSlot("router.storage.RouterV1");
        Storage storage router = _router();

        router.genesisBlock = Gear.newGenesis();
        router.implAddresses = Gear.AddressBook(_mirror, _wrappedVara, _middleware);
        router.validationSettings.thresholdNumerator = Gear.VALIDATORS_THRESHOLD_NUMERATOR;
        router.validationSettings.thresholdDenominator = Gear.VALIDATORS_THRESHOLD_DENOMINATOR;
        router.computeSettings = Gear.defaultComputationSettings();
        router.timelines = Gear.Timelines(_eraDuration, _electionDuration, _validationDelay);

        // Set validators for the era 0.
        _resetValidators(
            router.validationSettings.validators0,
            _aggregatedPublicKey,
            _verifiableSecretSharingCommitment,
            _validators,
            block.timestamp
        );
    }

    /// @custom:oz-upgrades-validate-as-initializer
    function reinitialize() public reinitializer(2) {
        __Ownable_init(owner());

        Storage storage oldRouter = _router();

        _setStorageSlot("router.storage.RouterV2");
        Storage storage newRouter = _router();

        // Set current block as genesis.
        newRouter.genesisBlock = Gear.newGenesis();

        // New router latestCommittedBlock is already zeroed.

        // Copy impl addresses from the old router.
        newRouter.implAddresses = oldRouter.implAddresses;

        // Copy signing threshold fraction from the old router.
        newRouter.validationSettings.thresholdNumerator = oldRouter.validationSettings.thresholdNumerator;
        newRouter.validationSettings.thresholdDenominator = oldRouter.validationSettings.thresholdDenominator;

        // Copy validators from the old router.
        // TODO #4557: consider what to do. Maybe we should start reelection process.
        // Skipping validators1 copying - means we forget election results
        // if an election is already done for the next era.
        _resetValidators(
            newRouter.validationSettings.validators0,
            Gear.currentEraValidators(oldRouter).aggregatedPublicKey,
            SSTORE2.read(Gear.currentEraValidators(oldRouter).verifiableSecretSharingCommitmentPointer),
            Gear.currentEraValidators(oldRouter).list,
            block.timestamp
        );

        // Copy computation settings from the old router.
        newRouter.computeSettings = oldRouter.computeSettings;

        // Copy timelines from the old router.
        newRouter.timelines = oldRouter.timelines;

        // All protocol data must be removed - so leave it zeroed in new router.
    }

    // # Views.

    /// @dev Returns the storage view of the contract storage.
    function storageView() public view returns (StorageView memory) {
        Storage storage router = _router();
        Gear.ValidationSettingsView memory validationSettings = Gear.toView(router.validationSettings);
        return StorageView({
            genesisBlock: router.genesisBlock,
            latestCommittedBatch: router.latestCommittedBatch,
            implAddresses: router.implAddresses,
            validationSettings: validationSettings,
            computeSettings: router.computeSettings,
            timelines: router.timelines,
            programsCount: router.protocolData.programsCount,
            validatedCodesCount: router.protocolData.validatedCodesCount
        });
    }

    function genesisBlockHash() public view returns (bytes32) {
        return _router().genesisBlock.hash;
    }

    function genesisTimestamp() public view returns (uint48) {
        return _router().genesisBlock.timestamp;
    }

    function latestCommittedBatchHash() public view returns (bytes32) {
        return _router().latestCommittedBatch.hash;
    }

    function latestCommittedBatchTimestamp() public view returns (uint48) {
        return _router().latestCommittedBatch.timestamp;
    }

    function mirrorImpl() public view returns (address) {
        return _router().implAddresses.mirror;
    }

    function wrappedVara() public view returns (address) {
        return _router().implAddresses.wrappedVara;
    }

    function middleware() public view returns (address) {
        return _router().implAddresses.middleware;
    }

    function validatorsAggregatedPublicKey() public view returns (Gear.AggregatedPublicKey memory) {
        return Gear.currentEraValidators(_router()).aggregatedPublicKey;
    }

    function validatorsVerifiableSecretSharingCommitment() external view returns (bytes memory) {
        return SSTORE2.read(Gear.currentEraValidators(_router()).verifiableSecretSharingCommitmentPointer);
    }

    function areValidators(address[] calldata _validators) public view returns (bool) {
        Gear.Validators storage _currentValidators = Gear.currentEraValidators(_router());

        for (uint256 i = 0; i < _validators.length; i++) {
            if (!_currentValidators.map[_validators[i]]) {
                return false;
            }
        }

        return true;
    }

    function isValidator(address _validator) public view returns (bool) {
        return Gear.currentEraValidators(_router()).map[_validator];
    }

    function signingThresholdFraction() public view returns (uint128, uint128) {
        IRouter.Storage storage router = _router();
        return (router.validationSettings.thresholdNumerator, router.validationSettings.thresholdDenominator);
    }

    function validators() public view returns (address[] memory) {
        return Gear.currentEraValidators(_router()).list;
    }

    function validatorsCount() public view returns (uint256) {
        return Gear.currentEraValidators(_router()).list.length;
    }

    function validatorsThreshold() public view returns (uint256) {
        IRouter.Storage storage router = _router();
        return Gear.validatorsThreshold(
            Gear.currentEraValidators(router).list.length,
            router.validationSettings.thresholdNumerator,
            router.validationSettings.thresholdDenominator
        );
    }

    function computeSettings() public view returns (Gear.ComputationSettings memory) {
        return _router().computeSettings;
    }

    function codeState(bytes32 _codeId) public view returns (Gear.CodeState) {
        return _router().protocolData.codes[_codeId];
    }

    function codesStates(bytes32[] calldata _codesIds) public view returns (Gear.CodeState[] memory) {
        Storage storage router = _router();

        Gear.CodeState[] memory res = new Gear.CodeState[](_codesIds.length);

        for (uint256 i = 0; i < _codesIds.length; i++) {
            res[i] = router.protocolData.codes[_codesIds[i]];
        }

        return res;
    }

    function programCodeId(address _programId) public view returns (bytes32) {
        return _router().protocolData.programs[_programId];
    }

    function programsCodeIds(address[] calldata _programsIds) public view returns (bytes32[] memory) {
        Storage storage router = _router();

        bytes32[] memory res = new bytes32[](_programsIds.length);

        for (uint256 i = 0; i < _programsIds.length; i++) {
            res[i] = router.protocolData.programs[_programsIds[i]];
        }

        return res;
    }

    function programsCount() public view returns (uint256) {
        return _router().protocolData.programsCount;
    }

    function validatedCodesCount() public view returns (uint256) {
        return _router().protocolData.validatedCodesCount;
    }

    function timelines() public view returns (Gear.Timelines memory) {
        return _router().timelines;
    }

    // Owner calls.
    function setMirror(address newMirror) external onlyOwner {
        _router().implAddresses.mirror = newMirror;
    }

    // # Calls.
    function lookupGenesisHash() external {
        Storage storage router = _router();

        require(router.genesisBlock.hash == bytes32(0), GenesisHashAlreadySet());

        bytes32 genesisHash = blockhash(router.genesisBlock.number);

        require(genesisHash != bytes32(0), GenesisHashNotFound());

        router.genesisBlock.hash = blockhash(router.genesisBlock.number);
    }

    function requestCodeValidation(bytes32 _codeId) external {
        require(blobhash(0) != 0, BlobNotFound());

        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), RouterGenesisHashNotInitialized());

        require(router.protocolData.codes[_codeId] == Gear.CodeState.Unknown, CodeAlreadyOnValidationOrValidated());

        router.protocolData.codes[_codeId] = Gear.CodeState.ValidationRequested;

        emit CodeValidationRequested(_codeId);
    }

    function createProgram(bytes32 _codeId, bytes32 _salt, address _overrideInitializer) external returns (address) {
        address mirror = _createProgram(_codeId, _salt, true);

        IMirror(mirror)
            .initialize(_overrideInitializer == address(0) ? msg.sender : _overrideInitializer, mirrorImpl(), true);

        return mirror;
    }

    function createProgramWithAbiInterface(
        bytes32 _codeId,
        bytes32 _salt,
        address _overrideInitializer,
        address _abiInterface
    ) external returns (address) {
        address mirror = _createProgram(_codeId, _salt, false);

        IMirror(mirror)
            .initialize(_overrideInitializer == address(0) ? msg.sender : _overrideInitializer, _abiInterface, false);

        return mirror;
    }

    // Before delegate call: 2426 (+ calldata size dependent) (+ delegate call gas)

    function commitBatch(
        Gear.BatchCommitment calldata _batch,
        Gear.SignatureType _signatureType,
        bytes[] calldata _signatures
    ) external nonReentrant {
        // num_words = 3 (already charged for initialization)
        //
        // during initialization `mstore(0x40, 0x80)`, `[0x40; 0x60)` - free memory pointer
        // so, initial memory size is 3 words: `(0x60 / 0x20)`

        // gas used: ~1135 (entrypoint, non-reentrant check)
        // emit DebugEvent(COMMIT_BATCH_BEFORE_LOADING_STORAGE);
        Storage storage router = _router();
        // gas used: ~2144 (load storage)
        // emit DebugEvent(COMMIT_BATCH_AFTER_LOADING_STORAGE);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_GENESIS_HASH);
        require(router.genesisBlock.hash != bytes32(0), RouterGenesisHashNotInitialized());
        // gas used: ~2134 (check genesis block hash)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_GENESIS_HASH);

        // `router.reserved` is always `0` but can be overridden in an RPC request
        // to estimate gas excluding `Gear.blockIsPredecessor()`.

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_RESERVED);
        if (router.reserved == 0) {
            // gas used: ~2129 (check reserved)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_RESERVED);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_BLOCK_IS_PREDECESSOR);
            require(Gear.blockIsPredecessor(_batch.blockHash, _batch.expiry), PredecessorBlockNotFound());
            // gas used: ~27798 (check block is predecessor, it's assumed that `_batch.expiry = type(uint8).max`)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_BLOCK_IS_PREDECESSOR);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_BATCH_TIMESTAMP);
            require(block.timestamp > _batch.blockTimestamp, BatchTimestampNotInPast());
            // gas used: ~106 (check batch timestamp)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_BATCH_TIMESTAMP);
        }

        // Check that batch correctly references to the previous committed batch.
        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_PREVIOUS_COMMITTED_BATCH_HASH);
        require(
            router.latestCommittedBatch.hash == _batch.previousCommittedBatchHash, InvalidPreviousCommittedBatchHash()
        );
        // gas used: ~2149 (check previous committed batch hash)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_PREVIOUS_COMMITTED_BATCH_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_BATCH_TIMESTAMP_TOO_EARLY);
        require(router.latestCommittedBatch.timestamp <= _batch.blockTimestamp, BatchTimestampTooEarly());
        // gas used: ~2216 (check batch timestamp too early)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_BATCH_TIMESTAMP_TOO_EARLY);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_TOO_MANY_CHAIN_COMMITMENTS);
        bytes32 _chainCommitmentHash = _commitChain(router, _batch);
        // emit DebugEvent(COMMIT_BATCH_AFTER_KECCAK256_CHAIN_COMMITMENT_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CODE_COMMITMENTS_ALLOCATION);
        bytes32 _codeCommitmentsHash = _commitCodes(router, _batch);
        // emit DebugEvent(COMMIT_BATCH_AFTER_KECCAK256_CODE_COMMITMENTS_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_NOT_BENCHMARKED_YET);
        bytes32 _rewardsCommitmentHash = _commitRewards(router, _batch);
        bytes32 _validatorsCommitmentHash = _commitValidators(router, _batch);
        // gas used: ~900 (not benchmarked yet, it's assumed that both commitments are empty)
        // emit DebugEvent(COMMIT_BATCH_AFTER_NOT_BENCHMARKED_YET);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_HASHING_BATCH);
        bytes32 _batchHash = Gear.batchCommitmentHash(
            _batch.blockHash,
            _batch.blockTimestamp,
            _batch.previousCommittedBatchHash,
            _batch.expiry,
            _chainCommitmentHash,
            _codeCommitmentsHash,
            _rewardsCommitmentHash,
            _validatorsCommitmentHash
        );
        // gas used: ~558 (hashing batch)
        // emit DebugEvent(COMMIT_BATCH_AFTER_HASHING_BATCH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_UPDATING_LATEST_COMMITTED_BATCH_HASH);
        router.latestCommittedBatch.hash = _batchHash;
        // gas used: ~2921 (updating latest committed batch hash)
        // emit DebugEvent(COMMIT_BATCH_AFTER_UPDATING_LATEST_COMMITTED_BATCH_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_UPDATING_LATEST_COMMITTED_BATCH_TIMESTAMP);
        router.latestCommittedBatch.timestamp = _batch.blockTimestamp;
        // gas used: ~3115 (updating latest committed batch timestamp)
        // emit DebugEvent(COMMIT_BATCH_AFTER_UPDATING_LATEST_COMMITTED_BATCH_TIMESTAMP);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_EMITTING_BATCH_COMMITTED);
        emit BatchCommitted(_batchHash);
        // gas used: ~1024 (emitting batch committed)
        // emit DebugEvent(COMMIT_BATCH_AFTER_EMITTING_BATCH_COMMITTED);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_ERA_STARTED_AT);
        require(
            Gear.validateSignaturesAt(
                router, TRANSIENT_STORAGE, _batchHash, _signatureType, _signatures, _batch.blockTimestamp
            ),
            SignatureVerificationFailed()
        );
        // gas used: ~40 (exit from validating signatures at)
        // emit DebugEvent(COMMIT_BATCH_AFTER_EXITING_VERIFY_AT_FUNCTION);
    }

    /* Helper private functions */

    function _createProgram(bytes32 _codeId, bytes32 _salt, bool _isSmall) private returns (address) {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), RouterGenesisHashNotInitialized());

        require(router.protocolData.codes[_codeId] == Gear.CodeState.Validated, CodeNotValidated());

        // Check for duplicate isn't necessary, because `Clones.cloneDeterministic`
        // reverts execution in case of address is already taken.
        bytes32 salt = Hashes.efficientKeccak256AsBytes32(_codeId, _salt);
        address actorId = _isSmall
            ? ClonesSmall.cloneDeterministic(address(this), salt)
            : Clones.cloneDeterministic(address(this), salt);

        router.protocolData.programs[actorId] = _codeId;
        router.protocolData.programsCount++;

        emit ProgramCreated(actorId, _codeId);

        return actorId;
    }

    function _commitChain(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(_batch.chainCommitment.length <= 1, TooManyChainCommitments());
        // gas used: ~240 (check too many chain commitments)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_TOO_MANY_CHAIN_COMMITMENTS);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_CHAIN_COMMITMENTS_LENGTH);
        if (_batch.chainCommitment.length == 0) {
            /// forge-lint: disable-next-line(asm-keccak256)
            return keccak256("");
        }
        // gas used: ~201 (check chain commitments length)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_CHAIN_COMMITMENTS_LENGTH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_TAKING_FIRST_CHAIN_COMMITMENT);
        Gear.ChainCommitment calldata _commitment = _batch.chainCommitment[0];
        // gas used: ~235 (take first chain commitment)
        // emit DebugEvent(COMMIT_BATCH_AFTER_TAKING_FIRST_CHAIN_COMMITMENT);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_TRANSITIONS_ALLOCATION);
        bytes32 _transitionsHash = _commitTransitions(router, _commitment.transitions);
        // emit DebugEvent(COMMIT_BATCH_AFTER_KECCAK256_TRANSITIONS_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_EMITTING_ANNOUNCES_COMMITTED);
        emit AnnouncesCommitted(_commitment.head);
        // gas used: ~1032 (emit announces committed)
        // emit DebugEvent(COMMIT_BATCH_AFTER_EMITTING_ANNOUNCES_COMMITTED);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_KECCAK256_CHAIN_COMMITMENT_HASH);
        return Gear.chainCommitmentHash(_transitionsHash, _commitment.head);
        // gas used: ~82 (keccak256 chain commitment hash)
    }

    function _commitCodes(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        // num_words += codeCommitmentsLen
        uint256 codeCommitmentsLen = _batch.codeCommitments.length;
        uint256 codeCommitmentsHashSize = codeCommitmentsLen * 32;
        uint256 codeCommitmentsPtr = Memory.allocate(codeCommitmentsHashSize);
        uint256 offset = 0;
        // gas used: ~373 + memory_cost (code commitments allocation)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CODE_COMMITMENTS_ALLOCATION);

        for (uint256 i = 0; i < codeCommitmentsLen; i++) {
            // emit DebugEvent(COMMIT_BATCH_BEFORE_TAKING_CODE_COMMITMENT);
            Gear.CodeCommitment calldata _commitment = _batch.codeCommitments[i];
            // gas used: ~215 (take code commitment)
            // emit DebugEvent(COMMIT_BATCH_AFTER_TAKING_CODE_COMMITMENT);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_FOR_INVALID_CODE_VALIDATION_STATE);
            require(
                router.protocolData.codes[_commitment.id] == Gear.CodeState.ValidationRequested,
                CodeValidationNotRequested()
            );
            // gas used: ~2270 (check for invalid code validation state)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_FOR_INVALID_CODE_VALIDATION_STATE);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_SETTING_VALIDATED_CODE_STATE);
            if (_commitment.valid) {
                router.protocolData.codes[_commitment.id] = Gear.CodeState.Validated;
                // gas used: ~3215 (setting validated code state)
                // emit DebugEvent(COMMIT_BATCH_AFTER_SETTING_VALIDATED_CODE_STATE);

                // emit DebugEvent(COMMIT_BATCH_BEFORE_INCREMENTING_VALIDATED_CODES_COUNT);
                router.protocolData.validatedCodesCount++;
                // gas used: ~22169 (incrementing validated codes count)
                // emit DebugEvent(COMMIT_BATCH_AFTER_INCREMENTING_VALIDATED_CODES_COUNT);
            } else {
                // emit DebugEvent(COMMIT_BATCH_BEFORE_DELETING_INVALID_CODE_STATE);
                delete router.protocolData.codes[_commitment.id];
                // gas used: ~3096 (deleting invalid code state)
                // emit DebugEvent(COMMIT_BATCH_AFTER_DELETING_INVALID_CODE_STATE);
            }

            // emit DebugEvent(COMMIT_BATCH_BEFORE_EMITTING_CODE_GOT_VALIDATED);
            emit CodeGotValidated(_commitment.id, _commitment.valid);
            // gas used: ~1484 (emit code got validated)
            // emit DebugEvent(COMMIT_BATCH_AFTER_EMITTING_CODE_GOT_VALIDATED);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_KECCAK256_CODE_COMMITMENT_HASH);
            bytes32 codeCommitmentHash = Gear.codeCommitmentHash(_commitment.id, _commitment.valid);
            // gas used: ~142 (keccak256 code commitment hash)
            // emit DebugEvent(COMMIT_BATCH_AFTER_KECCAK256_CODE_COMMITMENT_HASH);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_WRITING_CODE_COMMITMENT_HASH);
            Memory.writeWordAsBytes32(codeCommitmentsPtr, offset, codeCommitmentHash);
            // gas used: ~21 (write code commitment hash)
            // emit DebugEvent(COMMIT_BATCH_AFTER_WRITING_CODE_COMMITMENT_HASH);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_INCREMENTING_OFFSET2);
            unchecked {
                offset += 32;
            }
            // gas used: ~15 (increment offset)
            // emit DebugEvent(COMMIT_BATCH_AFTER_INCREMENTING_OFFSET2);
        }

        // emit DebugEvent(COMMIT_BATCH_BEFORE_KECCAK256_CODE_COMMITMENTS_HASH);
        return Hashes.efficientKeccak256AsBytes32(codeCommitmentsPtr, 0, codeCommitmentsHashSize);
        // keccak256_cost = 30 + 6 * codeCommitmentsLen
        // gas used: ~21 + keccak256_cost (hashing code commitments)
    }

    // TODO #4609
    // TODO #4611
    function _commitRewards(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(_batch.rewardsCommitment.length <= 1, TooManyRewardsCommitments());

        if (_batch.rewardsCommitment.length == 0) {
            /// forge-lint: disable-next-line(asm-keccak256)
            return keccak256("");
        }

        Gear.RewardsCommitment calldata _commitment = _batch.rewardsCommitment[0];

        require(_commitment.timestamp < _batch.blockTimestamp, RewardsCommitmentTimestampNotInPast());
        require(_commitment.timestamp >= router.genesisBlock.timestamp, RewardsCommitmentPredatesGenesis());

        uint256 commitmentEraIndex = Gear.eraIndexAt(router, _commitment.timestamp);
        uint256 batchEraIndex = Gear.eraIndexAt(router, _batch.blockTimestamp);

        require(commitmentEraIndex < batchEraIndex, RewardsCommitmentEraNotPrevious());

        address _middleware = router.implAddresses.middleware;
        bool success = IERC20(router.implAddresses.wrappedVara)
            .approve(_middleware, _commitment.operators.amount + _commitment.stakers.totalAmount);
        require(success, ApproveERC20Failed());

        bytes32 _operatorRewardsHash = IMiddleware(_middleware)
            .distributeOperatorRewards(
                router.implAddresses.wrappedVara, _commitment.operators.amount, _commitment.operators.root
            );

        bytes32 _stakerRewardsHash =
            IMiddleware(_middleware).distributeStakerRewards(_commitment.stakers, _commitment.timestamp);

        return Gear.rewardsCommitmentHash(_operatorRewardsHash, _stakerRewardsHash, _commitment.timestamp);
    }

    /// @dev Set validators for the next era.
    function _commitValidators(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(_batch.validatorsCommitment.length <= 1, TooManyValidatorsCommitments());

        if (_batch.validatorsCommitment.length == 0) {
            /// forge-lint: disable-next-line(asm-keccak256)
            return keccak256("");
        }

        Gear.ValidatorsCommitment calldata _commitment = _batch.validatorsCommitment[0];

        require(_commitment.validators.length > 0, EmptyValidatorsList());

        uint256 currentEraIndex = (block.timestamp - router.genesisBlock.timestamp) / router.timelines.era;

        require(_commitment.eraIndex == currentEraIndex + 1, CommitmentEraNotNext());

        uint256 nextEraStart = router.genesisBlock.timestamp + router.timelines.era * _commitment.eraIndex;
        require(block.timestamp >= nextEraStart - router.timelines.election, ElectionNotStarted());

        // Maybe free slot for new validators:
        Gear.Validators storage _validators = Gear.previousEraValidators(router);
        require(_validators.useFromTimestamp < block.timestamp, ValidatorsAlreadyScheduled());

        _resetValidators(
            _validators,
            _commitment.aggregatedPublicKey,
            _commitment.verifiableSecretSharingCommitment,
            _commitment.validators,
            nextEraStart
        );

        emit ValidatorsCommittedForEra(_commitment.eraIndex);

        return Gear.validatorsCommitmentHash(_commitment);
    }

    function _commitTransitions(Storage storage router, Gear.StateTransition[] calldata _transitions)
        private
        returns (bytes32)
    {
        // num_words += (1 + transitionsLen)
        // `+1` because free memory pointer at `0x80`

        uint256 transitionsLen = _transitions.length;
        uint256 transitionsHashSize = transitionsLen * 32;
        uint256 transitionsHashesMemPtr = Memory.allocate(transitionsHashSize);
        uint256 offset = 0;
        // gas used: ~366 + memory_cost (transitions allocation)
        // emit DebugEvent(COMMIT_BATCH_AFTER_TRANSITIONS_ALLOCATION);

        for (uint256 i = 0; i < transitionsLen; i++) {
            // emit DebugEvent(COMMIT_BATCH_BEFORE_TAKING_STATE_TRANSITION);
            Gear.StateTransition calldata transition = _transitions[i];
            // gas used: ~57 (take state transition)
            // emit DebugEvent(COMMIT_BATCH_AFTER_TAKING_STATE_TRANSITION);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_FOR_UNKNOWN_PROGRAM);
            require(router.protocolData.programs[transition.actorId] != 0, UnknownProgram());
            // gas used: ~2295/~295 (check for unknown program, gas depends on whether program is known or not)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_FOR_UNKNOWN_PROGRAM);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CALCULATING_VALUE_TO_RECEIVE);
            uint128 value = 0;

            if (transition.valueToReceive != 0 && !transition.valueToReceiveNegativeSign) {
                value = transition.valueToReceive;
            }
            // gas used: ~150/~327 (calculate value to receive, gas depends on whether value is zero or not)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CALCULATING_VALUE_TO_RECEIVE);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CALLING_PERFORM_STATE_TRANSITION);
            bytes32 transitionHash = IMirror(transition.actorId).performStateTransition{value: value}(transition);
            // num_words += ???
            // gas used: ~??? (call perform state transition)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CALLING_PERFORM_STATE_TRANSITION);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_WRITING_TRANSITION_HASH);
            Memory.writeWordAsBytes32(transitionsHashesMemPtr, offset, transitionHash);
            // gas used: ~20 (write transition hash)
            // emit DebugEvent(COMMIT_BATCH_AFTER_WRITING_TRANSITION_HASH);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_INCREMENTING_OFFSET1);
            unchecked {
                offset += 32;
            }
            // gas used: ~14 (increment offset)
            // emit DebugEvent(COMMIT_BATCH_AFTER_INCREMENTING_OFFSET1);
        }

        // emit DebugEvent(COMMIT_BATCH_BEFORE_KECCAK256_TRANSITIONS_HASH);
        return Hashes.efficientKeccak256AsBytes32(transitionsHashesMemPtr, 0, transitionsHashSize);
        // keccak256_cost = 30 + 6 * transitionsLen
        // gas used: ~11 + keccak256_cost (hashing state transitions)
    }

    function _resetValidators(
        Gear.Validators storage _validators,
        Gear.AggregatedPublicKey memory _newAggregatedPublicKey,
        bytes memory _verifiableSecretSharingCommitment,
        address[] memory _newValidators,
        uint256 _useFromTimestamp
    ) private {
        // basic checks for aggregated public key
        // but it probably should be checked with
        // [`frost_core::keys::PublicKeyPackage::{from_commitment, from_dkg_commitments}`]
        // https://docs.rs/frost-core/latest/frost_core/keys/struct.PublicKeyPackage.html#method.from_dkg_commitments
        // ideally onchain
        require(
            FROST.isValidPublicKey(_newAggregatedPublicKey.x, _newAggregatedPublicKey.y),
            InvalidFROSTAggregatedPublicKey()
        );
        _validators.aggregatedPublicKey = _newAggregatedPublicKey;
        _validators.verifiableSecretSharingCommitmentPointer = SSTORE2.write(_verifiableSecretSharingCommitment);
        for (uint256 i = 0; i < _validators.list.length; i++) {
            address _validator = _validators.list[i];
            _validators.map[_validator] = false;
        }
        for (uint256 i = 0; i < _newValidators.length; i++) {
            address _validator = _newValidators[i];
            _validators.map[_validator] = true;
        }
        _validators.list = _newValidators;
        _validators.useFromTimestamp = _useFromTimestamp;
    }

    function _router() private view returns (Storage storage router) {
        bytes32 slot = _getStorageSlot();

        assembly ("memory-safe") {
            router.slot := slot
        }
    }

    function _getStorageSlot() private view returns (bytes32) {
        return StorageSlot.getBytes32Slot(SLOT_STORAGE).value;
    }

    function _setStorageSlot(string memory namespace) private onlyOwner {
        bytes32 slot = SlotDerivation.erc7201Slot(namespace);
        StorageSlot.getBytes32Slot(SLOT_STORAGE).value = slot;

        emit StorageSlotChanged(slot);
    }

    receive() external payable {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), RouterGenesisHashNotInitialized());

        uint128 value = uint128(msg.value);
        require(value > 0, ZeroValueTransfer());

        address actorId = msg.sender;
        require(router.protocolData.programs[actorId] != 0, UnknownProgram());
    }
}
