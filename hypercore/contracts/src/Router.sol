// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {IProgram} from "./IProgram.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract Router {
    using ECDSA for bytes32;
    using MessageHashUtils for bytes32;

    uint256 public constant COUNT_OF_VALIDATORS = 1;
    uint256 public constant REQUIRED_SIGNATURES = 1;

    address public constant WRAPPED_VARA = 0x6377Bf194281FF2b14e807CC3740ac937744406f;

    address public owner;
    address public program;
    uint256 public countOfValidators;
    mapping(address => bool) public validators;
    mapping(bytes32 => bool) public codeIds;

    struct Transition {
        address actorId;
        bytes32 oldStateHash;
        bytes32 newStateHash;
    }

    event UploadCode(address origin, bytes32 codeId, bytes32 blobTx);

    event UploadedCode(bytes32 codeId);

    event CreateProgram(
        address origin, address actorId, bytes32 codeId, bytes32 salt, bytes initPayload, uint64 gasLimit, uint128 value
    );

    event UpdatedProgram(address actorId, bytes32 oldStateHash, bytes32 newStateHash);

    constructor() {
        owner = msg.sender;
    }

    modifier onlyOwner() {
        require(msg.sender == owner, "not owner");
        _;
    }

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
        emit UploadCode(tx.origin, codeId, blobTx);
    }

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata initPayload, uint64 gasLimit)
        external
        payable
    {
        require(codeIds[codeId], "unknown codeId");
        address actorId = Clones.cloneDeterministic(program, keccak256(abi.encodePacked(salt, codeId)), msg.value);
        IWrappedVara wrappedVara = IWrappedVara(WRAPPED_VARA);
        bool success = wrappedVara.transferFrom(msg.sender, address(this), wrappedVara.gasToValue(gasLimit));
        require(success, "failed to transfer tokens");
        emit CreateProgram(tx.origin, actorId, codeId, salt, initPayload, gasLimit, uint128(msg.value));
    }

    function commitCodes(bytes32[] calldata codeIdsArray, bytes[] calldata signatures) external onlyOwner {
        bytes memory message = abi.encodePacked(codeIdsArray);

        for (uint256 i = 0; i < codeIdsArray.length; i++) {
            bytes32 codeId = codeIdsArray[i];
            codeIds[codeId] = true;
            emit UploadedCode(codeId);
        }

        validateSignatures(message, signatures);
    }

    function commitTransitions(Transition[] calldata transitions, bytes[] calldata signatures) external onlyOwner {
        bytes memory message;

        for (uint256 i = 0; i < transitions.length; i++) {
            Transition calldata transition = transitions[i];
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
