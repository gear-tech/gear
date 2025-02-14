// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {FROST} from "frost-secp256k1-evm/FROST.sol";
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

    struct AggregatedPublicKey {
        uint256 x;
        uint256 y;
    }

    struct Validators {
        // TODO: After FROST multi signature applied - consider to remove validators map and list.
        // Replace it with list hash. Any node can access the list of validators using this hash from other nodes.
        AggregatedPublicKey aggregatedPublicKey;
        address verifiableSecretSharingCommitmentPointer;
        mapping(address => bool) map;
        address[] list;
        uint256 useFromTimestamp;
    }

    struct AddressBook {
        address mirror;
        address mirrorProxy;
        address wrappedVara;
    }

    struct CodeCommitment {
        bytes32 id;
        uint48 timestamp;
        bool valid;
    }

    struct BlockCommitment {
        bytes32 hash;
        uint48 timestamp;
        bytes32 previousCommittedBlock;
        bytes32 predecessorBlock;
        StateTransition[] transitions;
    }

    struct ValidatorsCommitment {
        AggregatedPublicKey aggregatedPublicKey;
        bytes verifiableSecretSharingCommitment;
        address[] validators;
        uint256 eraIndex;
    }

    struct BatchCommitment {
        CodeCommitment[] codeCommitments;
        BlockCommitment[] blockCommitments;
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

    struct Timelines {
        uint256 era;
        uint256 election;
        uint256 validationDelay;
    }

    struct ValidationSettings {
        uint16 signingThresholdPercentage;
        Validators validators0;
        Validators validators1;
    }

    struct ValueClaim {
        bytes32 messageId;
        address destination;
        uint128 value;
    }

    enum SignatureType {
        FROST,
        ECDSA
    }

    function validatorsCommitmentHash(Gear.ValidatorsCommitment memory commitment) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                commitment.aggregatedPublicKey.x,
                commitment.aggregatedPublicKey.y,
                commitment.validators,
                commitment.eraIndex
            )
        );
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
        for (uint256 i = block.number - 1; i > 0;) {
            bytes32 ret = blockhash(i);
            if (ret == hash) {
                return true;
            } else if (ret == 0) {
                break;
            }

            unchecked {
                i--;
            }
        }

        return false;
    }

    function codeCommitmentHash(CodeCommitment memory codeCommitment) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(codeCommitment.id, codeCommitment.timestamp, codeCommitment.valid));
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

    /// @dev Validates signatures of the given data hash.
    function validateSignatures(
        IRouter.Storage storage router,
        bytes32 _dataHash,
        Gear.SignatureType _signatureType,
        bytes[] calldata _signatures
    ) internal view returns (bool) {
        return validateSignaturesAt(router, _dataHash, _signatureType, _signatures, block.timestamp);
    }

    /// @dev Validates signatures of the given data hash at the given timestamp.
    function validateSignaturesAt(
        IRouter.Storage storage router,
        bytes32 _dataHash,
        SignatureType _signatureType,
        bytes[] calldata _signatures,
        uint256 ts
    ) internal view returns (bool) {
        uint256 eraStarted = eraStartedAt(router, block.timestamp);
        if (ts < eraStarted && block.timestamp < eraStarted + router.timelines.validationDelay) {
            require(ts >= router.genesisBlock.timestamp, "cannot validate before genesis");
            require(ts + router.timelines.era >= eraStarted, "timestamp is older than previous era");

            // Validation must be done using validators from previous era,
            // because `ts` is in the past and we are in the validation delay period.
        } else {
            require(ts <= block.timestamp, "timestamp cannot be in the future");

            if (ts < eraStarted) {
                ts = eraStarted;
            }

            // Validation must be done using current era validators.
        }

        Validators storage validators = validatorsAt(router, ts);
        bytes32 _messageHash = address(this).toDataWithIntendedValidatorHash(abi.encodePacked(_dataHash));

        if (_signatureType == SignatureType.FROST) {
            require(_signatures.length == 1, "FROST signature must be single");

            bytes memory _signature = _signatures[0];
            require(_signature.length == 96, "FROST signature length must be 96 bytes");

            uint256 _signatureRX;
            uint256 _signatureRY;
            uint256 _signatureZ;

            assembly ("memory-safe") {
                _signatureRX := mload(add(_signature, 0x20))
                _signatureRY := mload(add(_signature, 0x40))
                _signatureZ := mload(add(_signature, 0x60))
            }

            // extra security check (`FROST.verifySignature()` does not check public key validity)
            require(
                FROST.isValidPublicKey(validators.aggregatedPublicKey.x, validators.aggregatedPublicKey.y),
                "FROST aggregated public key is invalid"
            );

            return FROST.verifySignature(
                validators.aggregatedPublicKey.x,
                validators.aggregatedPublicKey.y,
                _signatureRX,
                _signatureRY,
                _signatureZ,
                _messageHash
            );
        } else if (_signatureType == SignatureType.ECDSA) {
            uint256 threshold =
                validatorsThreshold(validators.list.length, router.validationSettings.signingThresholdPercentage);

            uint256 validSignatures = 0;

            for (uint256 i = 0; i < _signatures.length; i++) {
                bytes calldata signature = _signatures[i];

                address validator = _messageHash.recover(signature);

                if (validators.map[validator]) {
                    if (++validSignatures == threshold) {
                        return true;
                    }
                }
            }

            return false;
        }

        return false;
    }

    function currentEraValidators(IRouter.Storage storage router) internal view returns (Validators storage) {
        return validatorsAt(router, block.timestamp);
    }

    /// @dev Returns previous era validators, if there is no previous era,
    /// then returns free validators slot, which must be zeroed.
    function previousEraValidators(IRouter.Storage storage router) internal view returns (Validators storage) {
        if (validatorsStoredInSlot1At(router, block.timestamp)) {
            return router.validationSettings.validators0;
        } else {
            return router.validationSettings.validators1;
        }
    }

    /// @dev Returns validators at the given timestamp.
    /// @param ts Timestamp for which to get the validators.
    function validatorsAt(IRouter.Storage storage router, uint256 ts) internal view returns (Validators storage) {
        if (validatorsStoredInSlot1At(router, ts)) {
            return router.validationSettings.validators1;
        } else {
            return router.validationSettings.validators0;
        }
    }

    /// @dev Returns whether validators at `ts` are stored in `router.validationSettings.validators1`.
    ///      `false` means that current era validators are stored in `router.validationSettings.validators0`.
    /// @param ts Timestamp for which to check the validators slot.
    function validatorsStoredInSlot1At(IRouter.Storage storage router, uint256 ts) internal view returns (bool) {
        uint256 ts0 = router.validationSettings.validators0.useFromTimestamp;
        uint256 ts1 = router.validationSettings.validators1.useFromTimestamp;

        // Impossible case, because of implementation.
        require(ts0 != ts1, "eras timestamp must not be equal");

        bool ts1Greater = ts0 < ts1;
        bool tsGE0 = ts0 <= ts;
        bool tsGE1 = ts1 <= ts;

        // Both eras are in the future - not supported by this function.
        require(tsGE0 || tsGE1, "could not identify validators for the given timestamp");

        // Two impossible cases, because of math rules:
        // 1)  ts1Greater && !tsGE0 &&  tsGE1
        // 2) !ts1Greater &&  tsGE0 && !tsGE1

        return ts1Greater && (tsGE0 == tsGE1);
    }

    function validatorsThreshold(uint256 validatorsAmount, uint16 thresholdPercentage)
        internal
        pure
        returns (uint256)
    {
        // Dividing by 10000 to adjust for percentage
        return (validatorsAmount * uint256(thresholdPercentage) + 9999) / 10000;
    }

    function valueClaimBytes(ValueClaim memory claim) internal pure returns (bytes memory) {
        return abi.encodePacked(claim.messageId, claim.destination, claim.value);
    }

    function eraIndexAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return (ts - router.genesisBlock.timestamp) / router.timelines.era;
    }

    function eraStartedAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return router.genesisBlock.timestamp + eraIndexAt(router, ts) * router.timelines.era;
    }
}
