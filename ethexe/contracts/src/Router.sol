// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {IRouter} from "./IRouter.sol";
import {IMirror} from "./IMirror.sol";
import {IWrappedVara} from "./IWrappedVara.sol";
import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";

contract Router is IRouter, OwnableUpgradeable, ReentrancyGuardTransient {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    // keccak256(abi.encode(uint256(keccak256("router.storage.Slot")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant SLOT_STORAGE = 0x5c09ca1b9b8127a4fd9f3c384aac59b661441e820e17733753ff5f2e86e1e000;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address initialOwner,
        address _mirror,
        address _mirrorProxy,
        address _wrappedVara,
        address[] memory _validatorsKeys
    ) public initializer {
        __Ownable_init(initialOwner);

        setStorageSlot("router.storage.RouterV1");
        Storage storage router = _getStorage();

        router.genesisBlockHash = blockhash(block.number - 1);
        router.mirror = _mirror;
        router.mirrorProxy = _mirrorProxy;
        router.wrappedVara = _wrappedVara;
        router.signingThresholdPercentage = 6666; // 2/3 percentage (66.66%).
        router.baseWeight = 2_500_000_000;
        router.valuePerWeight = 10;
        _setValidators(_validatorsKeys);
    }

    function reinitialize() public onlyOwner reinitializer(2) {
        Storage storage oldRouter = _getStorage();

        address _mirror = oldRouter.mirror;
        address _mirrorProxy = oldRouter.mirrorProxy;
        address _wrappedVara = oldRouter.wrappedVara;
        address[] memory _validatorsKeys = oldRouter.validatorsKeys;

        setStorageSlot("router.storage.RouterV2");
        Storage storage router = _getStorage();

        router.genesisBlockHash = blockhash(block.number - 1);
        router.mirror = _mirror;
        router.mirrorProxy = _mirrorProxy;
        router.wrappedVara = _wrappedVara;
        _setValidators(_validatorsKeys);
    }

    /* Operational functions */

    function getStorageSlot() public view returns (bytes32) {
        return StorageSlot.getBytes32Slot(SLOT_STORAGE).value;
    }

    function setStorageSlot(string memory namespace) public onlyOwner {
        bytes32 slot = keccak256(abi.encode(uint256(keccak256(bytes(namespace))) - 1)) & ~bytes32(uint256(0xff));

        StorageSlot.getBytes32Slot(SLOT_STORAGE).value = slot;

        emit StorageSlotChanged();
    }

    function genesisBlockHash() public view returns (bytes32) {
        Storage storage router = _getStorage();
        return router.genesisBlockHash;
    }

    function lastBlockCommitmentHash() public view returns (bytes32) {
        Storage storage router = _getStorage();
        return router.lastBlockCommitmentHash;
    }

    function wrappedVara() public view returns (address) {
        Storage storage router = _getStorage();
        return router.wrappedVara;
    }

    function mirrorProxy() public view returns (address) {
        Storage storage router = _getStorage();
        return router.mirrorProxy;
    }

    function mirror() public view returns (address) {
        Storage storage router = _getStorage();
        return router.mirror;
    }

    function setMirror(address _mirror) external onlyOwner {
        Storage storage router = _getStorage();
        router.mirror = _mirror;
    }

    /* Codes and programs observing functions */

    function validatedCodesCount() public view returns (uint256) {
        Storage storage router = _getStorage();
        return router.validatedCodesCount;
    }

    function codeState(bytes32 codeId) public view returns (CodeState) {
        Storage storage router = _getStorage();
        return router.codes[codeId];
    }

    function programsCount() public view returns (uint256) {
        Storage storage router = _getStorage();
        return router.programsCount;
    }

    function programCodeId(address program) public view returns (bytes32) {
        Storage storage router = _getStorage();
        return router.programs[program];
    }

    /* Validators' set related functions */

    function signingThresholdPercentage() public view returns (uint256) {
        Storage storage router = _getStorage();
        return router.signingThresholdPercentage;
    }

    function validatorsThreshold() public view returns (uint256) {
        // Dividing by 10000 to adjust for percentage
        return (validatorsCount() * signingThresholdPercentage() + 9999) / 10000;
    }

    function validatorsCount() public view returns (uint256) {
        Storage storage router = _getStorage();
        return router.validatorsKeys.length;
    }

    function validatorExists(address validator) public view returns (bool) {
        Storage storage router = _getStorage();
        return router.validators[validator];
    }

    function validators() public view returns (address[] memory) {
        Storage storage router = _getStorage();
        return router.validatorsKeys;
    }

    // TODO: replace `OnlyOwner` with `OnlyDAO` or smth.
    function updateValidators(address[] calldata validatorsAddressArray) external onlyOwner {
        _cleanValidators();
        _setValidators(validatorsAddressArray);

        emit ValidatorsSetChanged();
    }

    /* Economic and token related functions */

    function baseWeight() public view returns (uint64) {
        Storage storage router = _getStorage();
        return router.baseWeight;
    }

    function setBaseWeight(uint64 _baseWeight) external onlyOwner {
        Storage storage router = _getStorage();
        router.baseWeight = _baseWeight;

        emit BaseWeightChanged(_baseWeight);
    }

    function valuePerWeight() public view returns (uint128) {
        Storage storage router = _getStorage();
        return router.valuePerWeight;
    }

    function setValuePerWeight(uint128 _valuePerWeight) external onlyOwner {
        Storage storage router = _getStorage();
        router.valuePerWeight = _valuePerWeight;

        emit ValuePerWeightChanged(_valuePerWeight);
    }

    function baseFee() public view returns (uint128) {
        return uint128(baseWeight()) * valuePerWeight();
    }

    /* Primary Gear logic */

    function requestCodeValidation(bytes32 codeId, bytes32 blobTxHash) external {
        require(blobTxHash != 0 || blobhash(0) != 0, "blobTxHash couldn't be found");

        Storage storage router = _getStorage();

        require(router.codes[codeId] == CodeState.Unknown, "code with such id already requested or validated");

        router.codes[codeId] = CodeState.ValidationRequested;

        emit CodeValidationRequested(codeId, blobTxHash);
    }

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata payload, uint128 _value)
        external
        payable
        returns (address)
    {
        (address actorId, uint128 executableBalance) = _createProgramWithoutMessage(codeId, salt, _value);

        IMirror(actorId).initMessage(tx.origin, payload, _value, executableBalance);

        return actorId;
    }

    function createProgramWithDecoder(
        address decoderImplementation,
        bytes32 codeId,
        bytes32 salt,
        bytes calldata payload,
        uint128 _value
    ) external payable returns (address) {
        (address actorId, uint128 executableBalance) = _createProgramWithoutMessage(codeId, salt, _value);

        IMirror mirrorInstance = IMirror(actorId);

        mirrorInstance.createDecoder(decoderImplementation, keccak256(abi.encodePacked(codeId, salt)));

        mirrorInstance.initMessage(tx.origin, payload, _value, executableBalance);

        return actorId;
    }

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external {
        Storage storage router = _getStorage();

        bytes memory codeCommetmentsHashes;

        for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
            CodeCommitment calldata codeCommitment = codeCommitmentsArray[i];

            bytes32 codeCommitmentHash = _codeCommitmentHash(codeCommitment);

            codeCommetmentsHashes = bytes.concat(codeCommetmentsHashes, codeCommitmentHash);

            bytes32 codeId = codeCommitment.id;
            require(router.codes[codeId] == CodeState.ValidationRequested, "code should be requested for validation");

            if (codeCommitment.valid) {
                router.codes[codeId] = CodeState.Validated;
                router.validatedCodesCount++;

                emit CodeGotValidated(codeId, true);
            } else {
                delete router.codes[codeId];

                emit CodeGotValidated(codeId, false);
            }
        }

        _validateSignatures(keccak256(codeCommetmentsHashes), signatures);
    }

    function commitBlocks(BlockCommitment[] calldata blockCommitmentsArray, bytes[] calldata signatures)
        external
        nonReentrant
    {
        bytes memory blockCommitmentsHashes;

        for (uint256 i = 0; i < blockCommitmentsArray.length; i++) {
            BlockCommitment calldata blockCommitment = blockCommitmentsArray[i];

            bytes32 blockCommitmentHash = _commitBlock(blockCommitment);

            blockCommitmentsHashes = bytes.concat(blockCommitmentsHashes, blockCommitmentHash);
        }

        _validateSignatures(keccak256(blockCommitmentsHashes), signatures);
    }

    /* Helper private functions */

    function _createProgramWithoutMessage(bytes32 codeId, bytes32 salt, uint128 _value)
        private
        returns (address, uint128)
    {
        Storage storage router = _getStorage();

        require(router.codes[codeId] == CodeState.Validated, "code must be validated before program creation");

        uint128 baseFeeValue = baseFee();
        uint128 executableBalance = baseFeeValue * 10;

        uint128 totalValue = baseFeeValue + executableBalance + _value;

        _retrieveValue(totalValue);

        // Check for duplicate isn't necessary, because `Clones.cloneDeterministic`
        // reverts execution in case of address is already taken.
        address actorId = Clones.cloneDeterministic(router.mirrorProxy, keccak256(abi.encodePacked(codeId, salt)));

        router.programs[actorId] = codeId;
        router.programsCount++;

        emit ProgramCreated(actorId, codeId);

        return (actorId, executableBalance);
    }

    function _validateSignatures(bytes32 dataHash, bytes[] calldata signatures) private view {
        Storage storage router = _getStorage();

        uint256 threshold = validatorsThreshold();

        bytes32 messageHash = address(this).toDataWithIntendedValidatorHash(abi.encodePacked(dataHash));
        uint256 validSignatures = 0;

        for (uint256 i = 0; i < signatures.length; i++) {
            bytes calldata signature = signatures[i];

            address validator = messageHash.recover(signature);

            if (router.validators[validator]) {
                if (++validSignatures == threshold) {
                    break;
                }
            } else {
                require(false, "incorrect signature");
            }
        }

        require(validSignatures >= threshold, "not enough valid signatures");
    }

    function _commitBlock(BlockCommitment calldata blockCommitment) private returns (bytes32) {
        Storage storage router = _getStorage();

        require(
            router.lastBlockCommitmentHash == blockCommitment.prevCommitmentHash, "invalid previous commitment hash"
        );
        require(_isPredecessorHash(blockCommitment.predBlockHash), "allowed predecessor block not found");

        /*
         * @dev SECURITY: this settlement should be performed before any other calls to avoid reentrancy.
         */
        router.lastBlockCommitmentHash = blockCommitment.blockHash;

        bytes memory transitionsHashes;

        for (uint256 i = 0; i < blockCommitment.transitions.length; i++) {
            StateTransition calldata stateTransition = blockCommitment.transitions[i];

            bytes32 transitionHash = _doStateTransition(stateTransition);

            transitionsHashes = bytes.concat(transitionsHashes, transitionHash);
        }

        emit BlockCommitted(blockCommitment.blockHash);

        return _blockCommitmentHash(
            blockCommitment.blockHash,
            blockCommitment.prevCommitmentHash,
            blockCommitment.predBlockHash,
            keccak256(transitionsHashes)
        );
    }

    function _isPredecessorHash(bytes32 hash) private view returns (bool) {
        for (uint256 i = block.number - 1; i > 0; i--) {
            bytes32 ret = blockhash(i);
            if (ret == hash) {
                return true;
            } else if (ret == 0) {
                break;
            }
        }
        return false;
    }

    function _doStateTransition(StateTransition calldata stateTransition) private returns (bytes32) {
        Storage storage router = _getStorage();

        require(router.programs[stateTransition.actorId] != 0, "couldn't perform transition for unknown program");

        IWrappedVara wrappedVaraActor = IWrappedVara(router.wrappedVara);
        wrappedVaraActor.transfer(stateTransition.actorId, stateTransition.valueToReceive);

        IMirror mirrorActor = IMirror(stateTransition.actorId);

        bytes memory valueClaimsBytes;

        for (uint256 i = 0; i < stateTransition.valueClaims.length; i++) {
            ValueClaim calldata valueClaim = stateTransition.valueClaims[i];

            valueClaimsBytes = bytes.concat(
                valueClaimsBytes, abi.encodePacked(valueClaim.messageId, valueClaim.destination, valueClaim.value)
            );

            mirrorActor.valueClaimed(valueClaim.messageId, valueClaim.destination, valueClaim.value);
        }

        bytes memory messagesHashes;

        for (uint256 i = 0; i < stateTransition.messages.length; i++) {
            OutgoingMessage calldata outgoingMessage = stateTransition.messages[i];

            messagesHashes = bytes.concat(messagesHashes, _outgoingMessageHash(outgoingMessage));

            if (outgoingMessage.replyDetails.to == 0) {
                mirrorActor.messageSent(
                    outgoingMessage.id, outgoingMessage.destination, outgoingMessage.payload, outgoingMessage.value
                );
            } else {
                mirrorActor.replySent(
                    outgoingMessage.destination,
                    outgoingMessage.payload,
                    outgoingMessage.value,
                    outgoingMessage.replyDetails.to,
                    outgoingMessage.replyDetails.code
                );
            }
        }

        mirrorActor.updateState(stateTransition.newStateHash);

        return _stateTransitionHash(
            stateTransition.actorId,
            stateTransition.newStateHash,
            stateTransition.valueToReceive,
            keccak256(valueClaimsBytes),
            keccak256(messagesHashes)
        );
    }

    function _blockCommitmentHash(
        bytes32 blockHash,
        bytes32 prevCommitmentHash,
        bytes32 predBlockHash,
        bytes32 transitionsHashesHash
    ) private pure returns (bytes32) {
        return keccak256(abi.encodePacked(blockHash, prevCommitmentHash, predBlockHash, transitionsHashesHash));
    }

    function _stateTransitionHash(
        address actorId,
        bytes32 newStateHash,
        uint128 valueToReceive,
        bytes32 valueClaimsHash,
        bytes32 messagesHashesHash
    ) private pure returns (bytes32) {
        return keccak256(abi.encodePacked(actorId, newStateHash, valueToReceive, valueClaimsHash, messagesHashesHash));
    }

    function _outgoingMessageHash(OutgoingMessage calldata outgoingMessage) private pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                outgoingMessage.id,
                outgoingMessage.destination,
                outgoingMessage.payload,
                outgoingMessage.value,
                outgoingMessage.replyDetails.to,
                outgoingMessage.replyDetails.code
            )
        );
    }

    function _codeCommitmentHash(CodeCommitment calldata codeCommitment) private pure returns (bytes32) {
        return keccak256(abi.encodePacked(codeCommitment.id, codeCommitment.valid));
    }

    function _retrieveValue(uint128 _value) private {
        Storage storage router = _getStorage();

        bool success = IERC20(router.wrappedVara).transferFrom(tx.origin, address(this), _value);

        require(success, "failed to retrieve WVara");
    }

    function _cleanValidators() private {
        Storage storage router = _getStorage();

        for (uint256 i = 0; i < router.validatorsKeys.length; i++) {
            address validator = router.validatorsKeys[i];
            delete router.validators[validator];
        }

        delete router.validatorsKeys;
    }

    function _setValidators(address[] memory _validatorsArray) private {
        Storage storage router = _getStorage();

        require(router.validatorsKeys.length == 0, "previous validators weren't removed");

        for (uint256 i = 0; i < _validatorsArray.length; i++) {
            address validator = _validatorsArray[i];
            router.validators[validator] = true;
        }

        router.validatorsKeys = _validatorsArray;
    }

    function _getStorage() private view returns (Storage storage router) {
        bytes32 slot = getStorageSlot();

        /// @solidity memory-safe-assembly
        assembly {
            router.slot := slot
        }
    }
}
