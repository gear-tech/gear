// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IProgram} from "./IProgram.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract Router is Ownable {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    uint256 public constant COUNT_OF_VALIDATORS = 1;
    uint256 public constant REQUIRED_SIGNATURES = 1;

    address public constant WRAPPED_VARA = 0x6377Bf194281FF2b14e807CC3740ac937744406f;

    address public program;
    uint256 public countOfValidators;

    mapping(address => bool) public programs;
    mapping(address => bool) public validators;
    mapping(bytes32 => CodeState) public codes;

    enum CodeState {
        Unknown,
        Unconfirmed,
        Confirmed
    }

    struct CodeCommitment {
        bytes32 codeId;
        uint8 approved;
    }

    struct Transition {
        address actorId;
        bytes32 oldStateHash;
        bytes32 newStateHash;
    }

    event UploadCode(address origin, bytes32 codeId, bytes32 blobTx);

    event CodeApproved(bytes32 codeId);

    event CodeRejected(bytes32 codeId);

    event CreateProgram(
        address origin, address actorId, bytes32 codeId, bytes32 salt, bytes initPayload, uint64 gasLimit, uint128 value
    );

    event UpdatedProgram(address actorId, bytes32 oldStateHash, bytes32 newStateHash);

    event SendMessage(address origin, address destination, bytes payload, uint64 gasLimit, uint128 value);

    event SendReply(address origin, bytes32 replyToId, bytes payload, uint64 gasLimit, uint128 value);

    event ClaimValue(address origin, bytes32 messageId);

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
        emit CreateProgram(tx.origin, actorId, codeId, salt, initPayload, gasLimit, uint128(msg.value));
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

    function commitTransitions(Transition[] calldata transitions, bytes[] calldata signatures) external {
        bytes memory message;

        for (uint256 i = 0; i < transitions.length; i++) {
            Transition calldata transition = transitions[i];
            require(programs[transition.actorId], "unknown program");
            message = bytes.concat(
                message, abi.encodePacked(transition.actorId, transition.oldStateHash, transition.newStateHash)
            );
            IProgram(transition.actorId).performStateTransition(transition.oldStateHash, transition.newStateHash);
            emit UpdatedProgram(transition.actorId, transition.oldStateHash, transition.newStateHash);
        }

        validateSignatures(message, signatures);
    }

    function validateSignatures(bytes memory message, bytes[] calldata signatures) private view {
        bytes32 messageHash = keccak256(message).toEthSignedMessageHash();
        uint256 k = 0;

        for (; k < signatures.length; k++) {
            bytes calldata signature = signatures[k];
            address validator = messageHash.recover(signature);
            require(validators[validator], "unknown signature");
        }

        require(k >= REQUIRED_SIGNATURES, "not enough signatures");
    }
}
