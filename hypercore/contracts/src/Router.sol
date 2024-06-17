// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {IProgram} from "./IProgram.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract Router is Ownable, ReentrancyGuardTransient {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    uint256 public constant COUNT_OF_VALIDATORS = 1;
    uint256 public constant REQUIRED_SIGNATURES = 1;

    address public constant WRAPPED_VARA = 0x6377Bf194281FF2b14e807CC3740ac937744406f;

    address public program;
    uint256 public countOfValidators;
    mapping(address => bool) public validators;
    mapping(bytes32 => CodeState) public codes;
    mapping(address => bool) public programs;

    enum CodeState {
        Unknown,
        Unconfirmed,
        Confirmed
    }

    struct CodeCommitment {
        bytes32 codeId;
        uint8 approved;
    }

    struct ReplyDetails {
        bytes32 replyTo;
        bytes4 replyCode;
    }

    struct OutgoingMessage {
        address destination;
        bytes payload;
        uint128 value;
        ReplyDetails replyDetails;
    }

    struct TransitionCommitment {
        address actorId;
        bytes32 oldStateHash;
        bytes32 newStateHash;
        OutgoingMessage[] outgoingMessages;
    }

    event UploadCode(address indexed origin, bytes32 indexed codeId, bytes32 indexed blobTx);

    event CodeApproved(bytes32 indexed codeId);

    event CodeRejected(bytes32 indexed codeId);

    event CreateProgram(
        address indexed origin,
        address indexed actorId,
        bytes32 indexed codeId,
        bytes initPayload,
        uint64 gasLimit,
        uint128 value
    );

    event UpdatedProgram(address indexed actorId, bytes32 oldStateHash, bytes32 newStateHash);

    event UserMessageSent(address indexed destination, bytes payload, uint128 value);

    event UserReplySent(address indexed destination, bytes payload, uint128 value, bytes32 replyTo, bytes4 replyCode);

    event SendMessage(
        address indexed origin, address indexed destination, bytes payload, uint64 gasLimit, uint128 value
    );

    event SendReply(address indexed origin, bytes32 indexed replyToId, bytes payload, uint64 gasLimit, uint128 value);

    event ClaimValue(address indexed origin, bytes32 indexed messageId);

    constructor(address initialOwner) Ownable(initialOwner) {}

    function setProgram(address _program) external onlyOwner {
        require(program == address(0), "program already set");
        program = _program;
    }

    function addValidators(address[] calldata validatorsArray) external onlyOwner {
        uint256 newCountOfValidators = countOfValidators + validatorsArray.length;
        require(newCountOfValidators <= COUNT_OF_VALIDATORS, "validator set is limited");
        countOfValidators = newCountOfValidators;

        for (uint256 i = 0; i < validatorsArray.length; i++) {
            address validator = validatorsArray[i];
            validators[validator] = true;
        }
    }

    function removeValidators(address[] calldata validatorsArray) external onlyOwner {
        for (uint256 i = 0; i < validatorsArray.length; i++) {
            address validator = validatorsArray[i];
            delete validators[validator];
        }
    }

    function uploadCode(bytes32 codeId, bytes32 blobTx) external {
        require(blobTx != 0 || blobhash(0) != 0, "invalid transaction");
        require(codes[codeId] == CodeState.Unknown, "code already uploaded");
        codes[codeId] = CodeState.Unconfirmed;
        emit UploadCode(tx.origin, codeId, blobTx);
    }

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata initPayload, uint64 gasLimit)
        external
        payable
    {
        require(codes[codeId] == CodeState.Confirmed, "code is unconfirmed");
        address actorId = Clones.cloneDeterministic(program, keccak256(abi.encodePacked(salt, codeId)), msg.value);
        programs[actorId] = true;
        chargeGas(gasLimit);
        emit CreateProgram(tx.origin, actorId, codeId, initPayload, gasLimit, uint128(msg.value));
    }

    modifier onlyProgram() {
        require(programs[msg.sender], "unknown program");
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
        IWrappedVara wrappedVara = IWrappedVara(WRAPPED_VARA);
        bool success = wrappedVara.transferFrom(tx.origin, address(this), wrappedVara.gasToValue(gas));
        require(success, "failed to transfer tokens");
    }

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external {
        bytes memory message;

        for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
            CodeCommitment calldata codeCommitment = codeCommitmentsArray[i];
            require(codeCommitment.approved < 2, "'approved' field should represent bool as uint8");

            message = bytes.concat(message, keccak256(abi.encodePacked(codeCommitment.codeId, codeCommitment.approved)));

            bytes32 codeId = codeCommitment.codeId;
            require(codes[codeId] == CodeState.Unconfirmed, "code should be uploaded, but unconfirmed");

            if (codeCommitment.approved == 1) {
                codes[codeId] = CodeState.Confirmed;
                emit CodeApproved(codeId);
            } else {
                delete codes[codeId];
                emit CodeRejected(codeId);
            }
        }

        validateSignatures(message, signatures);
    }

    function commitTransitions(TransitionCommitment[] calldata transitionsCommitmentsArray, bytes[] calldata signatures)
        external
        nonReentrant
    {
        bytes memory message;

        for (uint256 i = 0; i < transitionsCommitmentsArray.length; i++) {
            TransitionCommitment calldata transitionCommitment = transitionsCommitmentsArray[i];
            require(programs[transitionCommitment.actorId], "unknown program");

            bytes memory message1;
            IProgram _program = IProgram(transitionCommitment.actorId);

            for (uint256 j = 0; j < transitionCommitment.outgoingMessages.length; j++) {
                OutgoingMessage calldata outgoingMessage = transitionCommitment.outgoingMessages[j];
                message1 = bytes.concat(
                    message1,
                    keccak256(
                        abi.encodePacked(
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
                    emit UserMessageSent(outgoingMessage.destination, outgoingMessage.payload, outgoingMessage.value);
                } else {
                    emit UserReplySent(
                        outgoingMessage.destination,
                        outgoingMessage.payload,
                        outgoingMessage.value,
                        replyDetails.replyTo,
                        replyDetails.replyCode
                    );
                }
            }

            message = bytes.concat(
                message,
                keccak256(
                    abi.encodePacked(
                        transitionCommitment.actorId,
                        transitionCommitment.oldStateHash,
                        transitionCommitment.newStateHash,
                        keccak256(message1)
                    )
                )
            );

            _program.performStateTransition(transitionCommitment.oldStateHash, transitionCommitment.newStateHash);

            emit UpdatedProgram(
                transitionCommitment.actorId, transitionCommitment.oldStateHash, transitionCommitment.newStateHash
            );
        }

        validateSignatures(message, signatures);
    }

    function validateSignatures(bytes memory message, bytes[] calldata signatures) private view {
        bytes32 messageHash = address(this).toDataWithIntendedValidatorHash(abi.encodePacked(keccak256(message)));
        uint256 k = 0;

        for (; k < signatures.length; k++) {
            bytes calldata signature = signatures[k];
            address validator = messageHash.recover(signature);
            require(validators[validator], "unknown signature");
        }

        require(k >= REQUIRED_SIGNATURES, "not enough signatures");
    }
}
