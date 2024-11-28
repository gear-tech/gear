// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {IRouter} from "./IRouter.sol";
import {IMirror} from "./IMirror.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";
import {Gear} from "./libraries/Gear.sol";

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

    function createProgram(bytes32 _codeId, bytes32 _salt, bytes calldata _payload, uint128 _value)
        external
        returns (address)
    {
        (address actorId, uint128 executableBalance) = _createProgramWithoutMessage(_codeId, _salt, _value);

        IMirror(actorId).initMessage(msg.sender, _payload, _value, executableBalance);

        return actorId;
    }

    function createProgramWithDecoder(
        address _decoderImpl,
        bytes32 _codeId,
        bytes32 _salt,
        bytes calldata _payload,
        uint128 _value
    ) external returns (address) {
        (address actorId, uint128 executableBalance) = _createProgramWithoutMessage(_codeId, _salt, _value);

        IMirror mirrorInstance = IMirror(actorId);

        mirrorInstance.createDecoder(_decoderImpl, keccak256(abi.encodePacked(_codeId, _salt)));

        mirrorInstance.initMessage(msg.sender, _payload, _value, executableBalance);

        return actorId;
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

    function _createProgramWithoutMessage(bytes32 _codeId, bytes32 _salt, uint128 _value)
        private
        returns (address, uint128)
    {
        Storage storage router = _router();
        require(router.genesisBlock.hash != bytes32(0), "router genesis is zero; call `lookupGenesisHash()` first");

        require(
            router.protocolData.codes[_codeId] == Gear.CodeState.Validated,
            "code must be validated before program creation"
        );

        // By default get 10 WVara for executable balance.
        uint128 executableBalance = 10_000_000_000_000;

        _retrieveValue(router, executableBalance + _value);

        // Check for duplicate isn't necessary, because `Clones.cloneDeterministic`
        // reverts execution in case of address is already taken.
        address actorId =
            Clones.cloneDeterministic(router.implAddresses.mirrorProxy, keccak256(abi.encodePacked(_codeId, _salt)));

        router.protocolData.programs[actorId] = _codeId;
        router.protocolData.programsCount++;

        emit ProgramCreated(actorId, _codeId);

        return (actorId, executableBalance);
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

        bytes memory transitionsHashes;

        for (uint256 i = 0; i < _blockCommitment.transitions.length; i++) {
            Gear.StateTransition calldata stateTransition = _blockCommitment.transitions[i];

            transitionsHashes = bytes.concat(transitionsHashes, _doStateTransition(stateTransition));
        }

        emit BlockCommitted(_blockCommitment.hash);

        return Gear.blockCommitmentHash(
            _blockCommitment.hash,
            _blockCommitment.timestamp,
            _blockCommitment.previousCommittedBlock,
            _blockCommitment.predecessorBlock,
            keccak256(transitionsHashes)
        );
    }

    function _doStateTransition(Gear.StateTransition calldata _stateTransition) private returns (bytes32) {
        Storage storage router = _router();

        require(
            router.protocolData.programs[_stateTransition.actorId] != 0,
            "couldn't perform transition for unknown program"
        );

        IWrappedVara wrappedVaraActor = IWrappedVara(router.implAddresses.wrappedVara);
        wrappedVaraActor.transfer(_stateTransition.actorId, _stateTransition.valueToReceive);

        IMirror mirrorActor = IMirror(_stateTransition.actorId);

        bytes memory valueClaimsBytes;

        for (uint256 i = 0; i < _stateTransition.valueClaims.length; i++) {
            Gear.ValueClaim calldata claim = _stateTransition.valueClaims[i];

            mirrorActor.valueClaimed(claim.messageId, claim.destination, claim.value);

            valueClaimsBytes = bytes.concat(valueClaimsBytes, Gear.valueClaimBytes(claim));
        }

        bytes memory messagesHashes;

        for (uint256 i = 0; i < _stateTransition.messages.length; i++) {
            Gear.Message calldata message = _stateTransition.messages[i];

            if (message.replyDetails.to == 0) {
                mirrorActor.messageSent(message.id, message.destination, message.payload, message.value);
            } else {
                mirrorActor.replySent(
                    message.destination,
                    message.payload,
                    message.value,
                    message.replyDetails.to,
                    message.replyDetails.code
                );
            }

            messagesHashes = bytes.concat(messagesHashes, Gear.messageHash(message));
        }

        if (_stateTransition.inheritor != address(0)) {
            mirrorActor.setInheritor(_stateTransition.inheritor);
        }

        mirrorActor.updateState(_stateTransition.newStateHash);

        return Gear.stateTransitionHash(
            _stateTransition.actorId,
            _stateTransition.newStateHash,
            _stateTransition.inheritor,
            _stateTransition.valueToReceive,
            keccak256(valueClaimsBytes),
            keccak256(messagesHashes)
        );
    }

    function _retrieveValue(Storage storage router, uint128 _value) private {
        bool success = IERC20(router.implAddresses.wrappedVara).transferFrom(msg.sender, address(this), _value);

        require(success, "failed to retrieve WVara");
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
