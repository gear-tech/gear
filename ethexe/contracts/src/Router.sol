// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Clones} from "./libraries/Clones.sol";
import {ClonesSmall} from "./libraries/ClonesSmall.sol";
import {Gear} from "./libraries/Gear.sol";
import {SSTORE2} from "./libraries/SSTORE2.sol";
import {FROST} from "frost-secp256k1-evm/FROST.sol";
import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";
import {IMirror} from "./IMirror.sol";
import {IRouter} from "./IRouter.sol";
import {IMiddleware} from "./IMiddleware.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {
    ReentrancyGuardTransientUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract Router is IRouter, OwnableUpgradeable, ReentrancyGuardTransientUpgradeable {
    // keccak256(abi.encode(uint256(keccak256("router.storage.Slot")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant SLOT_STORAGE = 0x5c09ca1b9b8127a4fd9f3c384aac59b661441e820e17733753ff5f2e86e1e000;
    // keccak256(abi.encode(uint256(keccak256("router.storage.Transient")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant TRANSIENT_STORAGE = 0xf02b465737fa6045c2ff53fb2df43c66916ac2166fa303264668fb2f6a1d8c00;

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
        require(block.timestamp > 0, "current timestamp must be greater than 0");
        require(_electionDuration > 0, "election duration must be greater than 0");
        require(_eraDuration > _electionDuration, "era duration must be greater than election duration");
        // _validationDelay must be small enough,
        // in order to restrict old era validators to make commitments, which can damage the system.
        require(_validationDelay < (_eraDuration - _electionDuration) / 10, "validation delay is too big");

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

        require(router.genesisBlock.hash == bytes32(0), "genesis hash already set");

        bytes32 genesisHash = blockhash(router.genesisBlock.number);

        require(genesisHash != bytes32(0), "unable to lookup genesis hash");

        router.genesisBlock.hash = blockhash(router.genesisBlock.number);
    }

    function requestCodeValidation(bytes32 _codeId) external {
        require(blobhash(0) != 0, "blob can't be found, router expected EIP-4844 transaction with WASM blob");

        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        require(
            router.protocolData.codes[_codeId] == Gear.CodeState.Unknown,
            "given code id is already on validation or validated"
        );

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

    function commitBatch(
        Gear.BatchCommitment calldata _batch,
        Gear.SignatureType _signatureType,
        bytes[] calldata _signatures
    ) external nonReentrant {
        Storage storage router = _router();

        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        // `router.reserved` is always `0` but can be overridden in an RPC request
        // to estimate gas excluding `Gear.blockIsPredecessor()`.
        if (router.reserved == 0) {
            require(Gear.blockIsPredecessor(_batch.blockHash, _batch.expiry), "allowed predecessor block wasn't found");
            require(block.timestamp > _batch.blockTimestamp, "batch timestamp must be in the past");
        }

        // Check that batch correctly references to the previous committed batch.
        require(
            router.latestCommittedBatch.hash == _batch.previousCommittedBatchHash,
            "invalid previous committed batch hash"
        );

        require(
            router.latestCommittedBatch.timestamp <= _batch.blockTimestamp,
            "batch timestamp must be greater or equal to latest committed batch timestamp"
        );

        bytes32 _chainCommitmentHash = _commitChain(router, _batch);
        bytes32 _codeCommitmentsHash = _commitCodes(router, _batch);
        bytes32 _rewardsCommitmentHash = _commitRewards(router, _batch);
        bytes32 _validatorsCommitmentHash = _commitValidators(router, _batch);

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

        router.latestCommittedBatch = Gear.CommittedBatchInfo(_batchHash, _batch.blockTimestamp);
        emit BatchCommitted(_batchHash);

        require(
            Gear.validateSignaturesAt(
                router, TRANSIENT_STORAGE, _batchHash, _signatureType, _signatures, _batch.blockTimestamp
            ),
            "signatures verification failed"
        );
    }

    /* Helper private functions */

    function _createProgram(bytes32 _codeId, bytes32 _salt, bool _isSmall) private returns (address) {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        require(
            router.protocolData.codes[_codeId] == Gear.CodeState.Validated,
            "code must be validated before program creation"
        );

        // Check for duplicate isn't necessary, because `Clones.cloneDeterministic`
        // reverts execution in case of address is already taken.
        bytes32 salt = keccak256(abi.encodePacked(_codeId, _salt));
        address actorId = _isSmall
            ? ClonesSmall.cloneDeterministic(address(this), salt)
            : Clones.cloneDeterministic(address(this), salt);

        router.protocolData.programs[actorId] = _codeId;
        router.protocolData.programsCount++;

        emit ProgramCreated(actorId, _codeId);

        return actorId;
    }

    function _commitChain(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(_batch.chainCommitment.length <= 1, "chainCommitment could contain at most one commitment");

        if (_batch.chainCommitment.length == 0) {
            return keccak256("");
        }

        Gear.ChainCommitment calldata _commitment = _batch.chainCommitment[0];

        bytes32 _transitionsHash = commitTransitions(router, _commitment.transitions);

        emit AnnouncesCommitted(_commitment.head);

        return Gear.chainCommitmentHash(_transitionsHash, _commitment.head);
    }

    function _commitCodes(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        bytes memory _codeCommitmentHashes;

        for (uint256 i = 0; i < _batch.codeCommitments.length; i++) {
            Gear.CodeCommitment calldata _commitment = _batch.codeCommitments[i];

            require(
                router.protocolData.codes[_commitment.id] == Gear.CodeState.ValidationRequested,
                "code must be requested for validation to be committed"
            );

            if (_commitment.valid) {
                router.protocolData.codes[_commitment.id] = Gear.CodeState.Validated;
                router.protocolData.validatedCodesCount++;
            } else {
                delete router.protocolData.codes[_commitment.id];
            }

            emit CodeGotValidated(_commitment.id, _commitment.valid);

            _codeCommitmentHashes = bytes.concat(_codeCommitmentHashes, Gear.codeCommitmentHash(_commitment));
        }

        return keccak256(_codeCommitmentHashes);
    }

    // TODO #4609
    // TODO #4611
    function _commitRewards(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(
            _batch.rewardsCommitment.length <= 1, "rewards commitment must be empty or contains only one commitment"
        );

        if (_batch.rewardsCommitment.length == 0) {
            return keccak256("");
        }

        Gear.RewardsCommitment calldata _commitment = _batch.rewardsCommitment[0];

        require(_commitment.timestamp < _batch.blockTimestamp, "rewards commitment timestamp must be for the past");
        require(_commitment.timestamp >= router.genesisBlock.timestamp, "rewards commitment timestamp predates genesis");

        uint256 commitmentEraIndex = Gear.eraIndexAt(router, _commitment.timestamp);
        uint256 batchEraIndex = Gear.eraIndexAt(router, _batch.blockTimestamp);

        require(commitmentEraIndex < batchEraIndex, "rewards commitment must target previous era");

        address _middleware = router.implAddresses.middleware;
        IERC20(router.implAddresses.wrappedVara)
            .approve(_middleware, _commitment.operators.amount + _commitment.stakers.totalAmount);

        bytes32 _operatorRewardsHash = IMiddleware(_middleware)
            .distributeOperatorRewards(
                router.implAddresses.wrappedVara, _commitment.operators.amount, _commitment.operators.root
            );

        bytes32 _stakerRewardsHash =
            IMiddleware(_middleware).distributeStakerRewards(_commitment.stakers, _commitment.timestamp);

        return keccak256(abi.encodePacked(_operatorRewardsHash, _stakerRewardsHash, _commitment.timestamp));
    }

    /// @dev Set validators for the next era.
    function _commitValidators(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        require(
            _batch.validatorsCommitment.length <= 1,
            "validators commitment must be empty or contains only one commitment"
        );

        if (_batch.validatorsCommitment.length == 0) {
            return keccak256("");
        }

        Gear.ValidatorsCommitment calldata _commitment = _batch.validatorsCommitment[0];

        require(_commitment.validators.length > 0, "new validators list must not be empty");

        uint256 currentEraIndex = (block.timestamp - router.genesisBlock.timestamp) / router.timelines.era;

        require(_commitment.eraIndex == currentEraIndex + 1, "commitment era index is not next era index");

        uint256 nextEraStart = router.genesisBlock.timestamp + router.timelines.era * _commitment.eraIndex;
        require(block.timestamp >= nextEraStart - router.timelines.election, "election is not yet started");

        // Maybe free slot for new validators:
        Gear.Validators storage _validators = Gear.previousEraValidators(router);
        require(_validators.useFromTimestamp < block.timestamp, "looks like validators for next era are already set");

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

    function commitTransitions(Storage storage router, Gear.StateTransition[] calldata _transitions)
        internal
        returns (bytes32)
    {
        uint256 transitionsLen = _transitions.length;
        uint256 transitionsHashesMemPtr = Memory.allocate(transitionsLen * 32);

        uint256 offset = 0;

        for (uint256 i = 0; i < transitionsLen; i++) {
            Gear.StateTransition calldata transition = _transitions[i];

            require(
                router.protocolData.programs[transition.actorId] != 0, "couldn't perform transition for unknown program"
            );

            uint128 value = 0;

            if (transition.valueToReceive != 0 && !transition.valueToReceiveNegativeSign) {
                value = transition.valueToReceive;
            }

            bytes32 transitionHash = IMirror(transition.actorId).performStateTransition{value: value}(transition);

            transitionsHashes = bytes.concat(transitionsHashes, transitionHash);

            offset += 32;

        }

        return keccak256(transitionsHashes);
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
            "FROST aggregated public key is invalid"
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
        bytes32 slot = keccak256(abi.encode(uint256(keccak256(bytes(namespace))) - 1)) & ~bytes32(uint256(0xff));
        StorageSlot.getBytes32Slot(SLOT_STORAGE).value = slot;
    }

    receive() external payable {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        uint128 value = uint128(msg.value);
        require(value > 0, "zero value transfer is not allowed");

        address actorId = msg.sender;
        require(router.protocolData.programs[actorId] != 0, "couldn't receive Ether from unknown program");
    }
}
