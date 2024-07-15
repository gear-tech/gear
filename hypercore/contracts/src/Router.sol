// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {IRouter} from "./IRouter.sol";
import {IProgram} from "./IProgram.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract Router is IRouter, OwnableUpgradeable, ReentrancyGuardTransient {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    uint256 public constant COUNT_OF_VALIDATORS = 1;
    uint256 public constant REQUIRED_SIGNATURES = 1;

    // keccak256(abi.encode(uint256(keccak256("router.storage.Slot")) - 1)) & ~bytes32(uint256(0xff))
    bytes32 private constant SLOT_STORAGE = 0x5c09ca1b9b8127a4fd9f3c384aac59b661441e820e17733753ff5f2e86e1e000;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(
        address initialOwner,
        address _program,
        address _minimalProgram,
        address _wrappedVara,
        address[] memory validatorsArray
    ) public initializer {
        __Ownable_init(initialOwner);

        setStorageSlot("router.storage.Router");
        RouterStorage storage router = getRouterStorage();

        router.program = _program;
        router.minimalProgram = _minimalProgram;
        router.wrappedVara = _wrappedVara;
        router.genesisBlockHash = blockhash(block.number - 1);

        addValidators(validatorsArray);
    }

    function getRouterStorage() private view returns (RouterStorage storage router) {
        bytes32 slot = getStorageSlot();
        /// @solidity memory-safe-assembly
        assembly {
            router.slot := slot
        }
    }

    function getStorageSlot() public view returns (bytes32) {
        return StorageSlot.getBytes32Slot(SLOT_STORAGE).value;
    }

    function setStorageSlot(string memory namespace) public onlyOwner {
        bytes32 slot = keccak256(abi.encode(uint256(keccak256(bytes(namespace))) - 1)) & ~bytes32(uint256(0xff));
        StorageSlot.getBytes32Slot(SLOT_STORAGE).value = slot;
    }

    function program() external view returns (address) {
        RouterStorage storage router = getRouterStorage();
        return router.program;
    }

    function setProgram(address _program) external onlyOwner {
        RouterStorage storage router = getRouterStorage();
        router.program = _program;
    }

    function minimalProgram() external view returns (address) {
        RouterStorage storage router = getRouterStorage();
        return router.minimalProgram;
    }

    function wrappedVara() external view returns (address) {
        RouterStorage storage router = getRouterStorage();
        return router.wrappedVara;
    }

    function genesisBlockHash() external view returns (bytes32) {
        RouterStorage storage router = getRouterStorage();
        return router.genesisBlockHash;
    }

    function lastBlockCommitmentHash() external view returns (bytes32) {
        RouterStorage storage router = getRouterStorage();
        return router.lastBlockCommitmentHash;
    }

    function countOfValidators() external view returns (uint256) {
        RouterStorage storage router = getRouterStorage();
        return router.countOfValidators;
    }

    function validators(address validator) external view returns (bool) {
        RouterStorage storage router = getRouterStorage();
        return router.validators[validator];
    }

    function codes(bytes32 codeId) external view returns (CodeState) {
        RouterStorage storage router = getRouterStorage();
        return router.codes[codeId];
    }

    function programs(address _program) external view returns (bool) {
        RouterStorage storage router = getRouterStorage();
        return router.programs[_program];
    }

    function addValidators(address[] memory validatorsArray) public onlyOwner {
        RouterStorage storage router = getRouterStorage();

        uint256 newCountOfValidators = router.countOfValidators + validatorsArray.length;
        require(newCountOfValidators <= COUNT_OF_VALIDATORS, "validator set is limited");
        router.countOfValidators = newCountOfValidators;

        for (uint256 i = 0; i < validatorsArray.length; i++) {
            address validator = validatorsArray[i];
            router.validators[validator] = true;
        }
    }

    function removeValidators(address[] calldata validatorsArray) external onlyOwner {
        RouterStorage storage router = getRouterStorage();

        for (uint256 i = 0; i < validatorsArray.length; i++) {
            address validator = validatorsArray[i];
            delete router.validators[validator];
        }
    }

    function uploadCode(bytes32 codeId, bytes32 blobTx) external {
        require(blobTx != 0 || blobhash(0) != 0, "invalid transaction");

        RouterStorage storage router = getRouterStorage();
        require(router.codes[codeId] == CodeState.Unknown, "code already uploaded");
        router.codes[codeId] = CodeState.Unconfirmed;

        emit UploadCode(tx.origin, codeId, blobTx);
    }

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata initPayload, uint64 gasLimit)
        external
        payable
        returns (address)
    {
        RouterStorage storage router = getRouterStorage();
        require(router.codes[codeId] == CodeState.Confirmed, "code is unconfirmed");

        address actorId =
            Clones.cloneDeterministic(router.minimalProgram, keccak256(abi.encodePacked(salt, codeId)), msg.value);
        router.programs[actorId] = true;

        chargeGas(gasLimit);

        emit CreateProgram(tx.origin, actorId, codeId, initPayload, gasLimit, uint128(msg.value));

        return actorId;
    }

    modifier onlyProgram() {
        RouterStorage storage router = getRouterStorage();
        require(router.programs[msg.sender], "unknown program");
        _;
    }

    function sendMessage(address destination, bytes calldata payload, uint64 gasLimit, uint128 value)
        external
        onlyProgram
    {
        chargeGas(gasLimit);
        emit SendMessage(tx.origin, destination, payload, gasLimit, value);
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit, uint128 value)
        external
        onlyProgram
    {
        chargeGas(gasLimit);
        emit SendReply(tx.origin, replyToId, payload, gasLimit, value);
    }

    function claimValue(bytes32 messageId) external onlyProgram {
        emit ClaimValue(tx.origin, messageId);
    }

    function chargeGas(uint64 gas) private {
        RouterStorage storage router = getRouterStorage();

        IWrappedVara wrappedVaraToken = IWrappedVara(router.wrappedVara);

        bool success = wrappedVaraToken.transferFrom(tx.origin, address(this), wrappedVaraToken.gasToValue(gas));
        require(success, "failed to transfer tokens");
    }

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external {
        RouterStorage storage router = getRouterStorage();

        bytes memory codesBytes;

        for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
            CodeCommitment calldata codeCommitment = codeCommitmentsArray[i];
            codesBytes =
                bytes.concat(codesBytes, keccak256(abi.encodePacked(codeCommitment.codeId, codeCommitment.approved)));

            bytes32 codeId = codeCommitment.codeId;
            require(router.codes[codeId] == CodeState.Unconfirmed, "code should be uploaded, but unconfirmed");

            if (codeCommitment.approved) {
                router.codes[codeId] = CodeState.Confirmed;
                emit CodeApproved(codeId);
            } else {
                delete router.codes[codeId];
                emit CodeRejected(codeId);
            }
        }

        validateSignatures(codesBytes, signatures);
    }

    function commitBlocks(BlockCommitment[] calldata commitments, bytes[] calldata signatures) external nonReentrant {
        bytes memory commitmentsBytes;
        for (uint256 i = 0; i < commitments.length; i++) {
            BlockCommitment calldata commitment = commitments[i];
            commitmentsBytes = bytes.concat(commitmentsBytes, commitBlock(commitment));
        }

        validateSignatures(commitmentsBytes, signatures);
    }

    function commitBlock(BlockCommitment calldata commitment) private returns (bytes32) {
        RouterStorage storage router = getRouterStorage();

        require(
            router.lastBlockCommitmentHash == commitment.allowedPrevCommitmentHash, "invalid predecessor commitment"
        );
        require(isPredecessorHash(commitment.allowedPredBlockHash), "allowed predecessor not found");

        router.lastBlockCommitmentHash = commitment.blockHash;
        emit BlockCommitted(commitment.blockHash);

        bytes memory transitionsBytes;
        for (uint256 i = 0; i < commitment.transitions.length; i++) {
            StateTransition calldata transition = commitment.transitions[i];
            require(router.programs[transition.actorId], "unknown program");

            IProgram _program = IProgram(transition.actorId);

            bytes memory outgoingBytes;
            for (uint256 j = 0; j < transition.outgoingMessages.length; j++) {
                OutgoingMessage calldata outgoingMessage = transition.outgoingMessages[j];
                outgoingBytes = bytes.concat(
                    outgoingBytes,
                    keccak256(
                        abi.encodePacked(
                            outgoingMessage.messageId,
                            outgoingMessage.destination,
                            outgoingMessage.payload,
                            outgoingMessage.value,
                            outgoingMessage.replyDetails.replyTo,
                            outgoingMessage.replyDetails.replyCode
                        )
                    )
                );

                if (outgoingMessage.value > 0) {
                    _program.performPayout(outgoingMessage.destination, outgoingMessage.value);
                }

                ReplyDetails calldata replyDetails = outgoingMessage.replyDetails;
                if (replyDetails.replyTo == 0 && replyDetails.replyCode == 0) {
                    emit UserMessageSent(
                        outgoingMessage.messageId,
                        outgoingMessage.destination,
                        outgoingMessage.payload,
                        outgoingMessage.value
                    );
                } else {
                    emit UserReplySent(
                        outgoingMessage.messageId,
                        outgoingMessage.destination,
                        outgoingMessage.payload,
                        outgoingMessage.value,
                        replyDetails.replyTo,
                        replyDetails.replyCode
                    );
                }
            }

            transitionsBytes = bytes.concat(
                transitionsBytes,
                keccak256(
                    abi.encodePacked(
                        transition.actorId, transition.oldStateHash, transition.newStateHash, keccak256(outgoingBytes)
                    )
                )
            );

            if (transition.oldStateHash != transition.newStateHash) {
                _program.performStateTransition(transition.oldStateHash, transition.newStateHash);

                emit UpdatedProgram(transition.actorId, transition.oldStateHash, transition.newStateHash);
            }
        }

        return keccak256(
            abi.encodePacked(
                commitment.blockHash,
                commitment.allowedPredBlockHash,
                commitment.allowedPrevCommitmentHash,
                keccak256(transitionsBytes)
            )
        );
    }

    function isPredecessorHash(bytes32 hash) private view returns (bool) {
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

    function validateSignatures(bytes memory message, bytes[] calldata signatures) private view {
        RouterStorage storage router = getRouterStorage();

        bytes32 messageHash = address(this).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(message)));
        uint256 k = 0;

        for (; k < signatures.length; k++) {
            bytes calldata signature = signatures[k];
            address validator = messageHash.recover(signature);
            require(router.validators[validator], "unknown signature");
        }

        require(k >= REQUIRED_SIGNATURES, "not enough signatures");
    }
}
