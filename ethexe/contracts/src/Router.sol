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

    function commitBatch(
        Gear.BatchCommitment calldata _batch,
        Gear.SignatureType _signatureType,
        bytes[] calldata _signatures
    ) external nonReentrant {
        Storage storage router = _router();

        require(router.genesisBlock.hash != bytes32(0), RouterGenesisHashNotInitialized());

        // `router.reserved` is always `0` but can be overridden in an RPC request
        // to estimate gas excluding `Gear.blockIsPredecessor()`.
        if (router.reserved == 0) {
            require(Gear.blockIsPredecessor(_batch.blockHash, _batch.expiry), PredecessorBlockNotFound());
            require(block.timestamp > _batch.blockTimestamp, BatchTimestampNotInPast());
        }

        // Check that batch correctly references to the previous committed batch.
        require(
            router.latestCommittedBatch.hash == _batch.previousCommittedBatchHash, InvalidPreviousCommittedBatchHash()
        );

        require(router.latestCommittedBatch.timestamp <= _batch.blockTimestamp, BatchTimestampTooEarly());

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

        router.latestCommittedBatch.hash = _batchHash;
        router.latestCommittedBatch.timestamp = _batch.blockTimestamp;

        emit BatchCommitted(_batchHash);

        require(
            Gear.validateSignaturesAt(
                router, TRANSIENT_STORAGE, _batchHash, _signatureType, _signatures, _batch.blockTimestamp
            ),
            SignatureVerificationFailed()
        );
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

        if (_batch.chainCommitment.length == 0) {
            /// forge-lint: disable-next-line(asm-keccak256)
            return keccak256("");
        }

        Gear.ChainCommitment calldata _commitment = _batch.chainCommitment[0];

        bytes32 _transitionsHash = _commitTransitions(router, _commitment.transitions);

        emit AnnouncesCommitted(_commitment.head);

        return Gear.chainCommitmentHash(_transitionsHash, _commitment.head);
    }

    function _commitCodes(Storage storage router, Gear.BatchCommitment calldata _batch) private returns (bytes32) {
        uint256 codeCommitmentsLen = _batch.codeCommitments.length;
        uint256 codeCommitmentsHashSize = codeCommitmentsLen * 32;
        uint256 codeCommitmentsPtr = Memory.allocate(codeCommitmentsHashSize);
        uint256 offset = 0;

        for (uint256 i = 0; i < codeCommitmentsLen; i++) {
            Gear.CodeCommitment calldata _commitment = _batch.codeCommitments[i];

            require(
                router.protocolData.codes[_commitment.id] == Gear.CodeState.ValidationRequested,
                CodeValidationNotRequested()
            );

            if (_commitment.valid) {
                router.protocolData.codes[_commitment.id] = Gear.CodeState.Validated;
                router.protocolData.validatedCodesCount++;
            } else {
                delete router.protocolData.codes[_commitment.id];
            }

            emit CodeGotValidated(_commitment.id, _commitment.valid);

            bytes32 codeCommitmentHash = Gear.codeCommitmentHash(_commitment.id, _commitment.valid);
            Memory.writeWordAsBytes32(codeCommitmentsPtr, offset, codeCommitmentHash);
            unchecked {
                offset += 32;
            }
        }

        return Hashes.efficientKeccak256AsBytes32(codeCommitmentsPtr, 0, codeCommitmentsHashSize);
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
        uint256 transitionsLen = _transitions.length;
        uint256 transitionsHashSize = transitionsLen * 32;
        uint256 transitionsHashesMemPtr = Memory.allocate(transitionsHashSize);
        uint256 offset = 0;

        for (uint256 i = 0; i < transitionsLen; i++) {
            Gear.StateTransition calldata transition = _transitions[i];

            require(router.protocolData.programs[transition.actorId] != 0, UnknownProgram());

            uint128 value = 0;

            if (transition.valueToReceive != 0 && !transition.valueToReceiveNegativeSign) {
                value = transition.valueToReceive;
            }

            bytes32 transitionHash = IMirror(transition.actorId).performStateTransition{value: value}(transition);
            Memory.writeWordAsBytes32(transitionsHashesMemPtr, offset, transitionHash);
            unchecked {
                offset += 32;
            }
        }

        return Hashes.efficientKeccak256AsBytes32(transitionsHashesMemPtr, 0, transitionsHashSize);
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
