// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";
import {Clones} from "@openzeppelin/contracts/proxy/Clones.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {ReentrancyGuardTransient} from "@openzeppelin/contracts/utils/ReentrancyGuardTransient.sol";
import {IRouter} from "./IRouter.sol";
import {IProgram} from "./IProgram.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract Router is IRouter, Ownable, ReentrancyGuardTransient {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    uint256 public constant COUNT_OF_VALIDATORS = 1;
    uint256 public constant REQUIRED_SIGNATURES = 1;

    address public immutable program;
    address public immutable wrappedVara;
    bytes32 public immutable genesisBlockHash;

    uint256 public countOfValidators;
    bytes32 public lastBlockCommitmentHash;
    mapping(address => bool) public validators;
    mapping(bytes32 => CodeState) public codes;
    mapping(address => bool) public programs;

    constructor(address initialOwner, address _program, address _wrappedVara, address[] memory validatorsArray)
        Ownable(initialOwner)
    {
        program = _program;
        wrappedVara = _wrappedVara;
        genesisBlockHash = blockhash(block.number - 1);
        addValidators(validatorsArray);
    }

    function addValidators(address[] memory validatorsArray) public onlyOwner {
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
        returns (address)
    {
        require(codes[codeId] == CodeState.Confirmed, "code is unconfirmed");
        address actorId = Clones.cloneDeterministic(program, keccak256(abi.encodePacked(salt, codeId)), msg.value);
        programs[actorId] = true;
        chargeGas(gasLimit);
        emit CreateProgram(tx.origin, actorId, codeId, initPayload, gasLimit, uint128(msg.value));
        return actorId;
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
        IWrappedVara wrappedVaraToken = IWrappedVara(wrappedVara);
        bool success = wrappedVaraToken.transferFrom(tx.origin, address(this), wrappedVaraToken.gasToValue(gas));
        require(success, "failed to transfer tokens");
    }

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external {
        bytes memory message;

        for (uint256 i = 0; i < codeCommitmentsArray.length; i++) {
            CodeCommitment calldata codeCommitment = codeCommitmentsArray[i];
            message = bytes.concat(message, keccak256(abi.encodePacked(codeCommitment.codeId, codeCommitment.approved)));

            bytes32 codeId = codeCommitment.codeId;
            require(codes[codeId] == CodeState.Unconfirmed, "code should be uploaded, but unconfirmed");

            if (codeCommitment.approved) {
                codes[codeId] = CodeState.Confirmed;
                emit CodeApproved(codeId);
            } else {
                delete codes[codeId];
                emit CodeRejected(codeId);
            }
        }

        validateSignatures(message, signatures);
    }

    /**
     * @dev Check if any of the last 256 blocks (excluding the current one) has the given hash.
     * @param hash The hash to check for.
     * @return found True if the hash is found, false otherwise.
     */
    function checkHashIsPred(bytes32 hash) private view returns (bool found) {
        for (uint256 i = block.number - 1; i > 0; i--) {
            if (blockhash(i) == hash) {
                return true;
            }

            if (block.number - i >= 256) {
                break;
            }
        }
        return false;
    }

    function commitBlock(BlockCommitment calldata commitment) private returns (bytes32) {
        require(lastBlockCommitmentHash == commitment.allowedPrevCommitmentHash, "Invalid predecessor commitment");
        require(checkHashIsPred(commitment.allowedPredBlockHash), "Allowed predecessor not found");

        lastBlockCommitmentHash = commitment.blockHash;
        emit BlockCommitted(commitment.blockHash);

        bytes memory transitions_bytes;
        for (uint256 i = 0; i < commitment.transitions.length; i++) {
            StateTransition calldata transition = commitment.transitions[i];
            require(programs[transition.actorId], "unknown program");

            IProgram _program = IProgram(transition.actorId);

            bytes memory outgoing_bytes;
            for (uint256 j = 0; j < transition.outgoingMessages.length; j++) {
                OutgoingMessage calldata outgoingMessage = transition.outgoingMessages[j];
                outgoing_bytes = bytes.concat(
                    outgoing_bytes,
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

            transitions_bytes = bytes.concat(
                transitions_bytes,
                keccak256(
                    abi.encodePacked(
                        transition.actorId, transition.oldStateHash, transition.newStateHash, keccak256(outgoing_bytes)
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
                keccak256(transitions_bytes)
            )
        );
    }

    function commitBlocks(BlockCommitment[] calldata commitments, bytes[] calldata signatures) external nonReentrant {
        bytes memory commitments_bytes;
        for (uint256 i = 0; i < commitments.length; i++) {
            BlockCommitment calldata commitment = commitments[i];
            commitments_bytes = bytes.concat(commitments_bytes, commitBlock(commitment));
        }

        validateSignatures(commitments_bytes, signatures);
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
