// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {IRouter} from "../IRouter.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

library Gear {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    // 2.5 * 10^9 of gear gas.
    uint64 public constant COMPUTATION_THRESHOLD = 2_500_000_000;

    // 2/3; 66.(6)% of validators signatures to verify.
    uint16 public constant SIGNING_THRESHOLD_PERCENTAGE = 6666;

    // 10 WVara tokens per compute second.
    uint128 public constant WVARA_PER_SECOND = 10_000_000_000_000;

    struct AddressBook {
        address mirror;
        address mirrorProxy;
        address wrappedVara;
    }

    struct BlockCommitment {
        bytes32 hash;
        uint48 timestamp;
        bytes32 previousCommittedBlock;
        bytes32 predecessorBlock;
        StateTransition[] transitions;
    }

    struct CodeCommitment {
        bytes32 id;
        bool valid;
    }

    enum CodeState {
        Unknown,
        ValidationRequested,
        Validated
    }

    struct CommittedBlockInfo {
        bytes32 hash;
        uint48 timestamp;
    }

    struct ComputationSettings {
        uint64 threshold;
        uint128 wvaraPerSecond;
    }

    struct GenesisBlockInfo {
        bytes32 hash;
        uint32 number;
        uint48 timestamp;
    }

    struct Message {
        bytes32 id;
        address destination;
        bytes payload;
        uint128 value;
        ReplyDetails replyDetails;
    }

    struct ProtocolData {
        mapping(bytes32 => CodeState) codes;
        mapping(address => bytes32) programs;
        uint256 programsCount;
        uint256 validatedCodesCount;
    }

    struct ReplyDetails {
        bytes32 to;
        bytes4 code;
    }

    struct StateTransition {
        address actorId;
        bytes32 newStateHash;
        address inheritor;
        uint128 valueToReceive;
        ValueClaim[] valueClaims;
        Message[] messages;
    }

    struct ValidationSettings {
        uint16 signingThresholdPercentage;
        address[] validators;
        // TODO: replace with one single pubkey and validators amount.
        mapping(address => bool) validatorsKeyMap;
    }

    struct ValueClaim {
        bytes32 messageId;
        address destination;
        uint128 value;
    }

    function blockCommitmentHash(
        bytes32 hash,
        uint48 timestamp,
        bytes32 previousCommittedBlock,
        bytes32 predecessorBlock,
        bytes32 transitionsHashesHash
    ) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(hash, timestamp, previousCommittedBlock, predecessorBlock, transitionsHashesHash)
        );
    }

    function blockIsPredecessor(bytes32 hash) internal view returns (bool) {
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

    function codeCommitmentHash(CodeCommitment calldata codeCommitment) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(codeCommitment.id, codeCommitment.valid));
    }

    function defaultComputationSettings() internal pure returns (ComputationSettings memory) {
        return ComputationSettings(COMPUTATION_THRESHOLD, WVARA_PER_SECOND);
    }

    function messageHash(Message memory message) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                message.id,
                message.destination,
                message.payload,
                message.value,
                message.replyDetails.to,
                message.replyDetails.code
            )
        );
    }

    function newGenesis() internal view returns (GenesisBlockInfo memory) {
        return GenesisBlockInfo(bytes32(0), uint32(block.number), uint48(block.timestamp));
    }

    function stateTransitionHash(
        address actor,
        bytes32 newStateHash,
        address inheritor,
        uint128 valueToReceive,
        bytes32 valueClaimsHash,
        bytes32 messagesHashesHash
    ) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(actor, newStateHash, inheritor, valueToReceive, valueClaimsHash, messagesHashesHash)
        );
    }

    function validateSignatures(IRouter.Storage storage router, bytes32 _dataHash, bytes[] calldata _signatures)
        internal
        view
        returns (bool)
    {
        uint256 threshold = validatorsThresholdOf(router.validationSettings);

        bytes32 msgHash = address(this).toDataWithIntendedValidatorHash(abi.encodePacked(_dataHash));
        uint256 validSignatures = 0;

        for (uint256 i = 0; i < _signatures.length; i++) {
            bytes calldata signature = _signatures[i];

            address validator = msgHash.recover(signature);

            if (router.validationSettings.validatorsKeyMap[validator]) {
                if (++validSignatures == threshold) {
                    return true;
                }
            }
        }

        return false;
    }

    function validatorsThresholdOf(ValidationSettings storage settings) internal view returns (uint256) {
        // Dividing by 10000 to adjust for percentage
        return (settings.validators.length * uint256(settings.signingThresholdPercentage) + 9999) / 10000;
    }

    function valueClaimBytes(ValueClaim memory claim) internal pure returns (bytes memory) {
        return abi.encodePacked(claim.messageId, claim.destination, claim.value);
    }
}
