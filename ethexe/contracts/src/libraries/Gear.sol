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
        /// @dev Should be empty for non-replies or abi encoded `ReplyDetails` type.
        bytes replyDetails;
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
        /// @dev Must be empty if no inheritor is set, otherwise 20 bytes of an inheritor address.
        bytes inheritor;
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

    function decodePackedAddress(bytes memory data) internal pure returns (address) {
        require(data.length == 20, "Address must be 20 bytes long if exist");

        address addr;

        assembly ("memory-safe") {
            addr := mload(add(data, 20))
        }

        return addr;
    }

    function decodeReplyDetails(bytes memory data) internal pure returns (ReplyDetails memory) {
        require(data.length == 36, "ReplyDetails must be 36 bytes long if exist");

        bytes32 to;
        bytes4 code;

        // Decode the 'to' field (first 32 bytes)
        assembly ("memory-safe") {
            to := mload(add(data, 32))
        }

        // Decode the 'code' field (next 4 bytes)
        assembly ("memory-safe") {
            code := mload(add(data, 36))
        }

        return ReplyDetails(to, code);
    }

    function defaultComputationSettings() internal pure returns (ComputationSettings memory) {
        return ComputationSettings(COMPUTATION_THRESHOLD, WVARA_PER_SECOND);
    }

    // TODO (breathx): optimize to calldata within the pr (?). Same for inheritor.
    function messageHash(Message memory message) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(message.id, message.destination, message.payload, message.value, message.replyDetails)
        );
    }

    function newGenesis() internal view returns (GenesisBlockInfo memory) {
        return GenesisBlockInfo(bytes32(0), uint32(block.number), uint48(block.timestamp));
    }

    function stateTransitionHash(
        address actor,
        bytes32 newStateHash,
        bytes memory inheritor,
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
