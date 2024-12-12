// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {Gear} from "./libraries/Gear.sol";
import {IMirror} from "./IMirror.sol";
import {IMirrorDecoder} from "./IMirrorDecoder.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";

contract Router is IRouter, OwnableUpgradeable, ReentrancyGuardTransient {
    // keccak256(abi.encode(uint256(keccak256("router.storage.Slot")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant SLOT_STORAGE = 0x5c09ca1b9b8127a4fd9f3c384aac59b661441e820e17733753ff5f2e86e1e000;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address _owner,
        address _mirror,
        address _mirrorProxy,
        address _wrappedVara,
        address[] calldata _validators
    ) public initializer {
        __Ownable_init(_owner);

        _setStorageSlot("router.storage.RouterV1");
        Storage storage router = _router();

        router.genesisBlock = Gear.newGenesis();
        router.implAddresses = Gear.AddressBook(_mirror, _mirrorProxy, _wrappedVara);
        router.validationSettings.signingThresholdPercentage = Gear.SIGNING_THRESHOLD_PERCENTAGE;
        _setValidators(router, _validators);
        router.computeSettings = Gear.defaultComputationSettings();
    }

    function reinitialize() public onlyOwner reinitializer(2) {
        Storage storage oldRouter = _router();

        _setStorageSlot("router.storage.RouterV2");
        Storage storage newRouter = _router();

        newRouter.genesisBlock = Gear.newGenesis();
        newRouter.implAddresses = oldRouter.implAddresses;

        newRouter.validationSettings.signingThresholdPercentage =
            oldRouter.validationSettings.signingThresholdPercentage;
        _setValidators(newRouter, oldRouter.validationSettings.validators);

        newRouter.computeSettings = oldRouter.computeSettings;
    }

    // # Views.
    function genesisBlockHash() public view returns (bytes32) {
        return _router().genesisBlock.hash;
    }

    function latestCommittedBlockHash() public view returns (bytes32) {
        return _router().latestCommittedBlock.hash;
    }

    function mirrorImpl() public view returns (address) {
        return _router().implAddresses.mirror;
    }

    function mirrorProxyImpl() public view returns (address) {
        return _router().implAddresses.mirrorProxy;
    }

    function wrappedVara() public view returns (address) {
        return _router().implAddresses.wrappedVara;
    }

    function areValidators(address[] calldata _validators) public view returns (bool) {
        Storage storage router = _router();

        for (uint256 i = 0; i < _validators.length; i++) {
            if (!router.validationSettings.validatorsKeyMap[_validators[i]]) {
                return false;
            }
        }

        return true;
    }

    function isValidator(address _validator) public view returns (bool) {
        return _router().validationSettings.validatorsKeyMap[_validator];
    }

    function signingThresholdPercentage() public view returns (uint16) {
        return _router().validationSettings.signingThresholdPercentage;
    }

    function validators() public view returns (address[] memory) {
        return _router().validationSettings.validators;
    }

    function validatorsCount() public view returns (uint256) {
        return _router().validationSettings.validators.length;
    }

    function validatorsThreshold() public view returns (uint256) {
        return Gear.validatorsThresholdOf(_router().validationSettings);
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

    function requestCodeValidation(bytes32 _codeId, bytes32 _blobTxHash) external {
        require(_blobTxHash != 0 || blobhash(0) != 0, "blob can't be found");

        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        require(
            router.protocolData.codes[_codeId] == Gear.CodeState.Unknown,
            "given code id is already on validation or validated"
        );

        router.protocolData.codes[_codeId] = Gear.CodeState.ValidationRequested;

        emit CodeValidationRequested(_codeId, _blobTxHash);
    }

    function createProgram(bytes32 _codeId, bytes32 _salt) external returns (address) {
        address mirror = _createProgram(_codeId, _salt);

        IMirror(mirror).initialize(msg.sender, address(0));

        return mirror;
    }

    function createProgramWithDecoder(address _decoderImpl, bytes32 _codeId, bytes32 _salt)
        external
        returns (address)
    {
        address mirror = _createProgram(_codeId, _salt);
        address decoder = _createDecoder(_decoderImpl, keccak256(abi.encodePacked(_codeId, _salt)), mirror);

        IMirror(mirror).initialize(msg.sender, decoder);

        return mirror;
    }

    // # Validators calls.
    function commitCodes(Gear.CodeCommitment[] calldata _codeCommitments, bytes[] calldata _signatures) external {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        bytes memory codeCommitmentsHashes;

        for (uint256 i = 0; i < _codeCommitments.length; i++) {
            Gear.CodeCommitment calldata codeCommitment = _codeCommitments[i];

            require(
                router.protocolData.codes[codeCommitment.id] == Gear.CodeState.ValidationRequested,
                "code must be requested for validation to be committed"
            );

            if (codeCommitment.valid) {
                router.protocolData.codes[codeCommitment.id] = Gear.CodeState.Validated;
                router.protocolData.validatedCodesCount++;
            } else {
                delete router.protocolData.codes[codeCommitment.id];
            }

            emit CodeGotValidated(codeCommitment.id, codeCommitment.valid);

            codeCommitmentsHashes = bytes.concat(codeCommitmentsHashes, Gear.codeCommitmentHash(codeCommitment));
        }

        require(
            Gear.validateSignatures(router, keccak256(codeCommitmentsHashes), _signatures),
            "signatures verification failed"
        );
    }

    function commitBlocks(Gear.BlockCommitment[] calldata _blockCommitments, bytes[] calldata _signatures)
        external
        nonReentrant
    {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        bytes memory blockCommitmentsHashes;

        for (uint256 i = 0; i < _blockCommitments.length; i++) {
            Gear.BlockCommitment calldata blockCommitment = _blockCommitments[i];
            blockCommitmentsHashes = bytes.concat(blockCommitmentsHashes, _commitBlock(router, blockCommitment));
        }

        require(
            Gear.validateSignatures(router, keccak256(blockCommitmentsHashes), _signatures),
            "signatures verification failed"
        );
    }

    /* Helper private functions */

    function _createProgram(bytes32 _codeId, bytes32 _salt) private returns (address) {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        require(
            router.protocolData.codes[_codeId] == Gear.CodeState.Validated,
            "code must be validated before program creation"
        );

        // Check for duplicate isn't necessary, because `Clones.cloneDeterministic`
        // reverts execution in case of address is already taken.
        address actorId =
            Clones.cloneDeterministic(router.implAddresses.mirrorProxy, keccak256(abi.encodePacked(_codeId, _salt)));

        router.protocolData.programs[actorId] = _codeId;
        router.protocolData.programsCount++;

        emit ProgramCreated(actorId, _codeId);

        return actorId;
    }

    function _createDecoder(address _implementation, bytes32 _salt, address _mirror) private returns (address) {
        address decoder = Clones.cloneDeterministic(_implementation, _salt);

        IMirrorDecoder(decoder).initialize(_mirror);

        return decoder;
    }

    function _commitBlock(Storage storage router, Gear.BlockCommitment calldata _blockCommitment)
        private
        returns (bytes32)
    {
        require(
            router.latestCommittedBlock.hash == _blockCommitment.previousCommittedBlock,
            "invalid previous committed block hash"
        );

        require(Gear.blockIsPredecessor(_blockCommitment.predecessorBlock), "allowed predecessor block wasn't found");

        /*
         * @dev SECURITY: this settlement should be performed before any other calls to avoid reentrancy.
         */
        router.latestCommittedBlock = Gear.CommittedBlockInfo(_blockCommitment.hash, _blockCommitment.timestamp);

        bytes32 transitionsHashesHash = _commitTransitions(router, _blockCommitment.transitions);

        emit BlockCommitted(_blockCommitment.hash);

        return Gear.blockCommitmentHash(
            _blockCommitment.hash,
            _blockCommitment.timestamp,
            _blockCommitment.previousCommittedBlock,
            _blockCommitment.predecessorBlock,
            transitionsHashesHash
        );
    }

    function _commitTransitions(Storage storage router, Gear.StateTransition[] calldata _transitions)
        private
        returns (bytes32)
    {
        bytes memory transitionsHashes;

        for (uint256 i = 0; i < _transitions.length; i++) {
            Gear.StateTransition calldata transition = _transitions[i];

            require(
                router.protocolData.programs[transition.actorId] != 0, "couldn't perform transition for unknown program"
            );

            IWrappedVara(router.implAddresses.wrappedVara).transfer(transition.actorId, transition.valueToReceive);

            bytes32 transitionHash = IMirror(transition.actorId).performStateTransition(transition);

            transitionsHashes = bytes.concat(transitionsHashes, transitionHash);
        }

        return keccak256(transitionsHashes);
    }

    function _setValidators(Storage storage router, address[] memory _validators) private {
        require(router.validationSettings.validators.length == 0, "remove previous validators first");

        for (uint256 i = 0; i < _validators.length; i++) {
            router.validationSettings.validatorsKeyMap[_validators[i]] = true;
        }

        router.validationSettings.validators = _validators;
    }

    function _removeValidators(Storage storage router) private {
        for (uint256 i = 0; i < router.validationSettings.validators.length; i++) {
            delete router.validationSettings.validatorsKeyMap[router.validationSettings.validators[i]];
        }

        delete router.validationSettings.validators;
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
}
