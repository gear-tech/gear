// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {SlotDerivation} from "@openzeppelin/contracts/utils/SlotDerivation.sol";
import {TransientSlot} from "@openzeppelin/contracts/utils/TransientSlot.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {SafeCast} from "@openzeppelin/contracts/utils/math/SafeCast.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {FROST} from "frost-secp256k1-evm/FROST.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";
import {IRouter} from "src/IRouter.sol";

/**
 * @dev Library for core protocol utility functions for hashing, validation, and consensus-related logic.
 *
 *      It provides:
 *      - Canonical hashing functions for protocol entities (messages, value claims, batch/code commitments, state transitions)
 *      - Validator set management with era-based switching and threshold configuration
 *      - Signature verification support for FROST aggregated signatures and ECDSA threshold signatures
 *        with transient storage to prevent double counting of signatures
 *      - Era and timeline utilities for selecting correct validator sets based on timestamp
 *
 *      The library acts as a shared foundation for consensus, validation, and commitment verification
 *      across all protocol components.
 */
library Gear {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    using TransientSlot for *;
    using SlotDerivation for *;

    /* # Constants */

    /**
     * @dev The threshold for computation cost in gear gas.
     *      2.5 * (10 ** 9) of gear gas.
     */
    uint64 public constant COMPUTATION_THRESHOLD = 2_500_000_000;

    /**
     * @dev 2/3 of validators must sign the commitment for it to be valid.
     * @dev The validators threshold numerator.
     */
    uint128 public constant VALIDATORS_THRESHOLD_NUMERATOR = 2;
    /**
     * @dev The validators threshold denominator.
     */
    uint128 public constant VALIDATORS_THRESHOLD_DENOMINATOR = 3;

    /**
     * @dev The amount of WVara tokens to be paid per compute second.
     * 10 WVARA tokens per compute second.
     */
    uint128 public constant WVARA_PER_SECOND = 10_000_000_000_000;

    /* # Errors */

    /**
     * @dev Thrown when signature validation is attempted before the genesis block.
     */
    error ValidationBeforeGenesis();

    /**
     * @dev Thrown when the timestamp is older than the previous era.
     */
    error TimestampOlderThanPreviousEra();

    /**
     * @dev Thrown when the timestamp is in the future.
     */
    error TimestampInFuture();

    /**
     * @dev Thrown when the number of FROST signatures is invalid.
     */
    error InvalidFrostSignatureCount();

    /**
     * @dev Thrown when the length of a FROST signature is invalid, it must be exactly 96 bytes.
     */
    error InvalidFrostSignatureLength();

    /**
     * @dev Thrown when the timestamp of an era is equal to the timestamp of the previous era.
     *      Should never happen, because the implementation.
     */
    error ErasTimestampMustNotBeEqual();

    /**
     * @dev Thrown when no validators are found for a given timestamp.
     *      Should never happen, because the implementation.
     */
    error ValidatorsNotFoundForTimestamp();

    /* # Structs */

    /**
     * @dev Represents an aggregated public key.
     *      It checked with `FROST.isValidPublicKey(x, y)` in `Router._resetValidators(...)`,
     *      so we can be sure that it is valid.
     */
    struct AggregatedPublicKey {
        uint256 x;
        uint256 y;
    }

    /**
     * @dev Represents validators information.
     */
    struct Validators {
        // TODO: After FROST multi signature applied - consider to remove validators map and list.
        // Replace it with list hash. Any node can access the list of validators using this hash from other nodes.
        /**
         * @dev The aggregated public key of validators (will be used soon for signature verification).
         */
        AggregatedPublicKey aggregatedPublicKey;
        /**
         * @dev Pointer to verifiable secret sharing commitment in the commitment storage.
         *      SSTORE2 is used for storing verifiable secret sharing commitments,
         *      because they can be large and SSTORE2 is more gas efficient for large data.
         */
        address verifiableSecretSharingCommitmentPointer;
        /**
         * @dev Mapping of validator addresses to their status.
         *      It is used for quick lookup of validator status during signature verification.
         */
        mapping(address => bool) map;
        /**
         * @dev List of validator addresses.
         */
        address[] list;
        /**
         * @dev Timestamp from which the validators are active.
         */
        uint256 useFromTimestamp;
    }

    /**
     * @dev Represents view of validators information.
     */
    struct ValidatorsView {
        AggregatedPublicKey aggregatedPublicKey;
        address verifiableSecretSharingCommitmentPointer;
        address[] list;
        uint256 useFromTimestamp;
    }

    /**
     * @dev Represents address book information.
     */
    struct AddressBook {
        /**
         * @dev The address of the `Mirror` contract.
         */
        address mirror;
        /**
         * @dev The address of the `WrappedVara` contract.
         */
        address wrappedVara;
        /**
         * @dev The address of the `POAMiddleware` contract.
         */
        address middleware;
    }

    /**
     * @dev Represents code commitment.
     */
    struct CodeCommitment {
        /**
         * @dev Code ID, calculated as `gear_core::ids::CodeId::generate(code)`
         */
        bytes32 id;
        /**
         * @dev Indicates if the code ID is valid.
         */
        bool valid;
    }

    /**
     * @dev Represents chain commitment.
     */
    struct ChainCommitment {
        /**
         * @dev Transitions of program states, value and messages.
         */
        StateTransition[] transitions;
        /**
         * @dev Head of chain. Hash of the last block in chain.
         */
        bytes32 head;
    }

    /**
     * @dev Represents validators commitment.
     */
    struct ValidatorsCommitment {
        AggregatedPublicKey aggregatedPublicKey;
        bytes verifiableSecretSharingCommitment;
        address[] validators;
        uint256 eraIndex;
    }

    /**
     * @dev Represents batch commitment.
     */
    struct BatchCommitment {
        /**
         * @dev Hash of ethereum block for which the batch was created.
         */
        bytes32 blockHash;
        /**
         * @dev Timestamp of ethereum block for which this batch was created.
         */
        uint48 blockTimestamp;
        /**
         * @dev Hash of previously committed batch hash.
         */
        bytes32 previousCommittedBatchHash;
        /**
         * @dev Expiry in blocks since `blockHash`.
         *      if 1 - then valid only in child block
         *      if 2 - then valid in child and grandchild blocks
         *      ... etc.
         */
        uint8 expiry;
        /**
         * @dev Chain commitment (contains one or zero commitments)
         */
        ChainCommitment[] chainCommitment;
        /**
         * @dev Code commitments (any number of commitments)
         */
        CodeCommitment[] codeCommitments;
        /**
         * @dev Rewards commitment (contains one or zero commitments)
         */
        RewardsCommitment[] rewardsCommitment;
        /**
         * @dev Validators commitment (contains one or zero commitments)
         */
        ValidatorsCommitment[] validatorsCommitment;
    }

    /**
     * @dev Represents rewards commitment.
     */
    struct RewardsCommitment {
        OperatorRewardsCommitment operators;
        StakerRewardsCommitment stakers;
        uint48 timestamp;
    }

    /**
     * @dev Represents operator rewards commitment.
     */
    struct OperatorRewardsCommitment {
        uint256 amount;
        bytes32 root;
    }

    /**
     * @dev Represents staker rewards commitment.
     */
    struct StakerRewardsCommitment {
        StakerRewards[] distribution;
        uint256 totalAmount;
        address token;
    }

    /**
     * @dev Represents staker rewards.
     */
    struct StakerRewards {
        address vault;
        uint256 amount;
    }

    /**
     * @dev Represents the state of code commitment.
     */
    enum CodeState {
        /**
         * @dev The code commitment is in an unknown state (`CodeState.Unknown = 0 as uint8`).
         *      This is the default state for all code commitments,
         *      and it means that the code commitment has not been processed yet.
         */
        Unknown,
        /**
         * @dev The code commitment has requested validation by user (`CodeState.ValidationRequested = 1 as uint8`).
         *      Users calls `IRouter(router).requestCodeValidation(bytes32 _codeId)` to request code validation and
         *      attaches sidecar to this transaction (the transaction is encoded in EIP-7594 format),
         *      then validators can validate the code commitment and set `CodeState.Validated` in case of success.
         */
        ValidationRequested,
        /**
         * @dev The code commitment has been validated by validators (`CodeState.Validated = 2 as uint8`).
         */
        Validated
    }

    /**
     * @dev Represents information about committed batch.
     */
    struct CommittedBatchInfo {
        bytes32 hash;
        uint48 timestamp;
    }

    /**
     * @dev Represents computation settings.
     */
    struct ComputationSettings {
        uint64 threshold;
        uint128 wvaraPerSecond;
    }

    /**
     * @dev Represents information about genesis block.
     */
    struct GenesisBlockInfo {
        bytes32 hash;
        uint32 number;
        uint48 timestamp;
    }

    /**
     * @dev Represents message.
     */
    struct Message {
        /**
         * @dev Message ID, type in Rust: `gprimitives::MessageId`.
         */
        bytes32 id;
        /**
         * @dev The destination address for the message.
         *      This is actually `gprimtives::ActorId`, but we use `address` type in Solidity.
         */
        address destination;
        /**
         * @dev The payload of the message.
         */
        bytes payload;
        /**
         * @dev The value associated with the message.
         */
        uint128 value;
        // TODO (breathx): use ReplyDetails[]
        /**
         * @dev Details about the reply.
         *      If `replyDetails.to` is zero address, then it means that there is no reply for this message.
         */
        ReplyDetails replyDetails;
        /**
         * @dev Indicates whether the message is call or just simple message.
         *      For more information on how `call` flag works, please see implementation:
         *      - `Mirror._sendMailboxedMessage`
         *      - `Mirror._sendReplyMessage`
         */
        bool call;
    }

    /**
     * @dev Represents the protocol data.
     */
    struct ProtocolData {
        /**
         * @dev Mapping of code IDs to their validation states.
         *      By default, all code IDs are in `CodeState.Unknown` state, which means that they have not been processed yet.
         */
        mapping(bytes32 => CodeState) codes;
        /**
         * @dev Mapping of program addresses to their code IDs.
         *      This mapping can be used to ensure that the program (`Mirror`) exists.
         */
        mapping(address => bytes32) programs;
        /**
         * @dev The total number of programs (`Mirror` instances).
         */
        uint256 programsCount;
        /**
         * @dev The total number of validated codes. Used for fast-sync.
         */
        uint256 validatedCodesCount;
        /**
         * @dev The maximum number of validators for era.
         */
        uint16 maxValidators;
    }

    /**
     * @dev Represents details about reply.
     */
    struct ReplyDetails {
        /**
         * @dev Message id, this message replies on.
         *      In Rust it's `gprimitives::MessageId`, but we use `bytes32` type in Solidity.
         */
        bytes32 to;
        // TODO (breathx): consider struct and methods to determine reason.
        // TODO (breathx): consider avoid submitting auto replies.
        /**
         * @dev Reply code. In Rust it's `gear_core::message::ReplyCode` enum, but we use `bytes4` type in Solidity
         *      and encode it as 4 bytes.
         */
        bytes4 code;
    }

    /**
     * @dev Represents state transition of `Mirror`.
     *      Most important type in this, in Rust we use this type to mutate state of `Mirror` instances.
     *      (see `ethexe/common/src/gear.rs` for more details on how this type is used in Rust).
     */
    struct StateTransition {
        /**
         * @dev Actor ID for which the state transition is performed.
         *      In Rust it's `gprimitives::ActorId`, but we use `address` type in Solidity.
         */
        address actorId;
        /**
         * @dev The hash of the new state.
         *      In Rust it's `gprimitives::H256`, but we use `bytes32` type in Solidity.
         */
        bytes32 newStateHash;
        /**
         * @dev Indicates whether the actor has exited.
         */
        bool exited;
        /**
         * @dev The address of the inheritor.
         *      Inheritor specifies the address to which all available program value should be transferred.
         */
        address inheritor;
        /**
         * @dev Value to receive for the actor after the state transition.
         *      We represent `valueToReceive` as `uint128` and `bool` because each non-zero byte costs 16 gas,
         *      and each zero byte costs 4 gas (see https://evm.codes/about#gascosts).
         *      Also see `ethexe/common/src/gear.rs`.
         */
        uint128 valueToReceive;
        /**
         * @dev Indicates whether the `valueToReceive` is negative.
         *      if `false` - then the `Router` sends `valueToReceive` value to `Mirror`
         *      if `true` - then the `Mirror` sends `valueToReceive` value to `Router`
         */
        bool valueToReceiveNegativeSign;
        /**
         * @dev Array of value claims.
         */
        ValueClaim[] valueClaims;
        /**
         * @dev Array of messages.
         */
        Message[] messages;
    }

    /**
     * @dev Represents the timelines.
     */
    struct Timelines {
        uint256 era;
        uint256 election;
        uint256 validationDelay;
    }

    /**
     * @dev Represents the validation settings.
     */
    struct ValidationSettings {
        uint128 thresholdNumerator;
        uint128 thresholdDenominator;
        Validators validators0;
        Validators validators1;
    }

    /**
     * @dev Represents the view of validation settings.
     */
    struct ValidationSettingsView {
        uint128 thresholdNumerator;
        uint128 thresholdDenominator;
        ValidatorsView validators0;
        ValidatorsView validators1;
    }

    /**
     * @dev Represents claim for value.
     */
    struct ValueClaim {
        bytes32 messageId;
        address destination;
        uint128 value;
    }

    /**
     * @dev Represents the symbiotic contracts addresses.
     */
    struct SymbioticContracts {
        // Symbiotic Registries
        address vaultRegistry;
        address operatorRegistry;
        address networkRegistry;
        address middlewareService;
        address networkOptIn;
        address stakerRewardsFactory;
        // Symbiotic Gear contracts
        address operatorRewards;
        address roleSlashRequester;
        address roleSlashExecutor;
        address vetoResolver;
    }

    /**
     * @dev Represents the type of signature used.
     */
    enum SignatureType {
        FROST,
        ECDSA
    }

    /**
     * @dev Computes the hash of `ChainCommitment`.
     * @param _transitionsHash The hash of the transitions in the chain commitment.
     * @param _head The head of the chain commitment.
     */
    function chainCommitmentHash(bytes32 _transitionsHash, bytes32 _head) internal pure returns (bytes32) {
        return Hashes.efficientKeccak256AsBytes32(_transitionsHash, _head);
    }

    /**
     * @dev Computes the hash of `CodeCommitment`.
     * @param codeId The ID of the code.
     * @param valid The validation status of the code.
     */
    function codeCommitmentHash(bytes32 codeId, bool valid) internal pure returns (bytes32) {
        bytes32 _codeCommitmentHash;
        assembly ("memory-safe") {
            mstore(0x00, codeId)
            mstore8(0x20, valid)
            _codeCommitmentHash := keccak256(0x00, 0x21)
        }
        return _codeCommitmentHash;
    }

    /**
     * @dev Computes the hash of `RewardsCommitment`.
     * @param _operatorRewardsHash The hash of the operator rewards.
     * @param _stakerRewardsHash The hash of the staker rewards.
     * @param _timestamp The timestamp for the rewards commitment.
     */
    function rewardsCommitmentHash(bytes32 _operatorRewardsHash, bytes32 _stakerRewardsHash, uint48 _timestamp)
        internal
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(_operatorRewardsHash, _stakerRewardsHash, _timestamp));
    }

    /**
     * @dev Computes the hash of `ValidatorsCommitment`.
     * @param commitment The validators commitment.
     */
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

    /**
     * @dev Computes the hash of `BatchCommitment`.
     * @param _block The hash of the block.
     * @param _timestamp The timestamp for the batch commitment.
     * @param _prevCommittedBlock The hash of the previous committed block.
     * @param _expiry The expiry time for the batch commitment.
     * @param _chainCommitmentHash The hash of the chain commitment.
     * @param _codeCommitmentsHash The hash of the code commitments.
     * @param _rewardsCommitmentHash The hash of the rewards commitment.
     * @param _validatorsCommitmentHash The hash of the validators commitment.
     */
    function batchCommitmentHash(
        bytes32 _block,
        uint48 _timestamp,
        bytes32 _prevCommittedBlock,
        uint8 _expiry,
        bytes32 _chainCommitmentHash,
        bytes32 _codeCommitmentsHash,
        bytes32 _rewardsCommitmentHash,
        bytes32 _validatorsCommitmentHash
    ) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                _block,
                _timestamp,
                _prevCommittedBlock,
                _expiry,
                _chainCommitmentHash,
                _codeCommitmentsHash,
                _rewardsCommitmentHash,
                _validatorsCommitmentHash
            )
        );
    }

    /**
     * @dev Computes the hash of `Message`.
     * @param message The message for which to compute the hash.
     */
    function messageHash(Message memory message) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                message.id,
                message.destination,
                message.payload,
                message.value,
                message.replyDetails.to,
                message.replyDetails.code,
                message.call
            )
        );
    }

    /**
     * @dev Computes the hash of `ValueClaim`.
     * @param _messageId The message ID.
     * @param _destination The destination address.
     * @param _value The value of the claim.
     */
    function valueClaimHash(bytes32 _messageId, address _destination, uint128 _value) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(_messageId, _destination, _value));
    }

    /**
     * @dev Computes the hash of `StateTransition`.
     * @param actor The actor address.
     * @param newStateHash The hash of the new state.
     * @param exited The exit status.
     * @param inheritor The inheritor address.
     * @param valueToReceive The value to receive.
     * @param valueToReceiveNegativeSign The sign of the value to receive.
     * @param valueClaimsHash The hash of the value claims.
     * @param messagesHashesHash The hash of the messages hashes.
     */
    function stateTransitionHash(
        address actor,
        bytes32 newStateHash,
        bool exited,
        address inheritor,
        uint128 valueToReceive,
        bool valueToReceiveNegativeSign,
        bytes32 valueClaimsHash,
        bytes32 messagesHashesHash
    ) internal pure returns (bytes32) {
        return keccak256(
            abi.encodePacked(
                actor,
                newStateHash,
                exited,
                inheritor,
                valueToReceive,
                valueToReceiveNegativeSign,
                valueClaimsHash,
                messagesHashesHash
            )
        );
    }

    /**
     * @dev Checks if block is predecessor of the current block.
     * @param hash The hash of the block to check.
     * @param expiry The expiry time for the block.
     * @return isPredecessor `true` if the block is predecessor, `false` otherwise.
     */
    function blockIsPredecessor(bytes32 hash, uint8 expiry) internal view returns (bool isPredecessor) {
        uint256 start = block.number - 1;
        uint256 end = expiry >= block.number ? 0 : block.number - expiry;
        for (uint256 i = start; i >= end;) {
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

    /**
     * @dev Returns the default computation settings.
     * @return computationSettings The default computation settings.
     */
    function defaultComputationSettings() internal pure returns (ComputationSettings memory computationSettings) {
        return ComputationSettings({threshold: COMPUTATION_THRESHOLD, wvaraPerSecond: WVARA_PER_SECOND});
    }

    /**
     * @dev Creates new genesis block info.
     * @return genesisBlockInfo The new genesis block info.
     */
    function newGenesis() internal view returns (GenesisBlockInfo memory genesisBlockInfo) {
        return
            GenesisBlockInfo({hash: bytes32(0), number: SafeCast.toUint32(block.number), timestamp: Time.timestamp()});
    }

    /**
     * @dev Validates signatures of the given data hash at the given timestamp.
     * @param router The router storage.
     * @param routerTransientStorage The router transient storage slot for this validation.
     * @param _dataHash The hash of the data to validate signatures for.
     * @param _signatureType The type of signatures to validate.
     * @param _signatures The signatures to validate.
     * @param ts The timestamp at which to validate signatures.
     */
    function validateSignaturesAt(
        IRouter.Storage storage router,
        bytes32 routerTransientStorage,
        bytes32 _dataHash,
        SignatureType _signatureType,
        bytes[] calldata _signatures,
        uint256 ts
    ) internal returns (bool) {
        uint256 eraStarted = eraStartedAt(router, block.timestamp);
        if (ts < eraStarted && block.timestamp < eraStarted + router.timelines.validationDelay) {
            require(ts >= router.genesisBlock.timestamp, ValidationBeforeGenesis());
            require(ts + router.timelines.era >= eraStarted, TimestampOlderThanPreviousEra());

            // Validation must be done using validators from previous era,
            // because `ts` is in the past and we are in the validation delay period.
        } else {
            require(ts <= block.timestamp, TimestampInFuture());

            if (ts < eraStarted) {
                ts = eraStarted;
            }

            // Validation must be done using current era validators.
        }

        Validators storage validators = validatorsAt(router, ts);
        bytes32 _messageHash = address(this).toDataWithIntendedValidatorHash(_dataHash);

        if (_signatureType == SignatureType.FROST) {
            require(_signatures.length == 1, InvalidFrostSignatureCount());

            bytes memory _signature = _signatures[0];
            require(_signature.length == 96, InvalidFrostSignatureLength());

            uint256 _signatureCommitmentX;
            uint256 _signatureCommitmentY;
            uint256 _signatureZ;

            assembly ("memory-safe") {
                _signatureCommitmentX := mload(add(_signature, 0x20))
                _signatureCommitmentY := mload(add(_signature, 0x40))
                _signatureZ := mload(add(_signature, 0x60))
            }

            /**
             * @dev SECURITY: `FROST.isValidPublicKey(validators.aggregatedPublicKey.x, validators.aggregatedPublicKey.y)` is not called here,
             *      because it is already checked in `Router._resetValidators(...)`.
             */
            return FROST.verifySignature(
                validators.aggregatedPublicKey.x,
                validators.aggregatedPublicKey.y,
                _signatureCommitmentX,
                _signatureCommitmentY,
                _signatureZ,
                _messageHash
            );
        } else if (_signatureType == SignatureType.ECDSA) {
            uint256 threshold = validatorsThreshold(
                validators.list.length,
                router.validationSettings.thresholdNumerator,
                router.validationSettings.thresholdDenominator
            );

            uint256 validSignatures = 0;

            for (uint256 i = 0; i < _signatures.length; i++) {
                bytes calldata signature = _signatures[i];

                address validator = _messageHash.recover(signature);

                if (validators.map[validator]) {
                    /**
                     * @dev SECURITY:
                     *      We use transient storage to prevent multiple signatures from the same validator.
                     */
                    bytes32 transientStorageValidatorsSlot = routerTransientStorage.deriveMapping(validator);

                    if (transientStorageValidatorsSlot.asBoolean().tload()) {
                        continue;
                    } else {
                        transientStorageValidatorsSlot.asBoolean().tstore(true);
                    }

                    if (++validSignatures == threshold) {
                        return true;
                    }
                }
            }

            return false;
        }

        return false;
    }

    /**
     * @dev Returns the validators for the current era.
     * @param router The router storage.
     */
    function currentEraValidators(IRouter.Storage storage router) internal view returns (Validators storage) {
        return validatorsAt(router, block.timestamp);
    }

    /**
     * @dev Returns previous era validators, if there is no previous era,
     *      then returns free validators slot, which must be zeroed.
     * @param router The router storage.
     */
    function previousEraValidators(IRouter.Storage storage router) internal view returns (Validators storage) {
        if (validatorsStoredInSlot1At(router, block.timestamp)) {
            return router.validationSettings.validators0;
        } else {
            return router.validationSettings.validators1;
        }
    }

    /**
     * @dev Returns validators at the given timestamp.
     * @param ts Timestamp for which to get the validators.
     */
    function validatorsAt(IRouter.Storage storage router, uint256 ts) internal view returns (Validators storage) {
        if (validatorsStoredInSlot1At(router, ts)) {
            return router.validationSettings.validators1;
        } else {
            return router.validationSettings.validators0;
        }
    }

    /**
     * @dev Returns `true` if validators at `ts` are stored in `router.validationSettings.validators1`.
     *      `false` means that current era validators are stored in `router.validationSettings.validators0`.
     * @param ts Timestamp for which to check the validators slot.
     * @return isSlot1 Whether validators at `ts` are stored in `router.validationSettings.validators1`.
     */
    function validatorsStoredInSlot1At(IRouter.Storage storage router, uint256 ts)
        internal
        view
        returns (bool isSlot1)
    {
        uint256 ts0 = router.validationSettings.validators0.useFromTimestamp;
        uint256 ts1 = router.validationSettings.validators1.useFromTimestamp;

        // Impossible case, because of implementation.
        require(ts0 != ts1, ErasTimestampMustNotBeEqual());

        bool ts1Greater = ts0 < ts1;
        bool tsGe0 = ts0 <= ts;
        bool tsGe1 = ts1 <= ts;

        // Both eras are in the future - not supported by this function.
        require(tsGe0 || tsGe1, ValidatorsNotFoundForTimestamp());

        // Two impossible cases, because of math rules:
        // 1)  ts1Greater && !tsGe0 &&  tsGe1
        // 2) !ts1Greater &&  tsGe0 && !tsGe1

        return ts1Greater && (tsGe0 == tsGe1);
    }

    /**
     * @dev Calculates the threshold number of valid signatures required.
     *      The formula is:
     *      - `(validatorsAmount * thresholdNumerator).div_ceil(thresholdDenominator)`
     * @param validatorsAmount The total number of validators.
     * @param thresholdNumerator The numerator of the threshold fraction.
     * @param thresholdDenominator The denominator of the threshold fraction.
     * @return threshold The threshold number of valid signatures required.
     */
    function validatorsThreshold(uint256 validatorsAmount, uint128 thresholdNumerator, uint128 thresholdDenominator)
        internal
        pure
        returns (uint256 threshold)
    {
        uint256 a;
        unchecked {
            a = validatorsAmount * thresholdNumerator;
        }
        uint256 d = a / thresholdDenominator;
        uint256 r = a % thresholdDenominator;
        unchecked {
            return (r > 0) ? d + 1 : d;
        }
    }

    /**
     * @dev Returns the era index for the given timestamp.
     * @param router The router storage.
     * @param ts The timestamp for which to get the era index.
     */
    function eraIndexAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return (ts - router.genesisBlock.timestamp) / router.timelines.era;
    }

    /**
     * @dev Returns the timestamp when the era started for the given timestamp.
     * @param router The router storage.
     * @param ts The timestamp for which to get the era start timestamp.
     */
    function eraStartedAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return router.genesisBlock.timestamp + eraIndexAt(router, ts) * router.timelines.era;
    }

    /**
     * @dev Converts `Gear.Validators` storage to `Gear.ValidatorsView` struct.
     *      Note that `validators.map` is passed as `validators.list`.
     * @param validators The `Gear.Validators` storage to convert.
     * @return validatorsView `Gear.ValidatorsView` struct with the same data as the input storage.
     */
    function toView(Gear.Validators storage validators)
        internal
        view
        returns (Gear.ValidatorsView memory validatorsView)
    {
        return Gear.ValidatorsView({
            aggregatedPublicKey: validators.aggregatedPublicKey,
            verifiableSecretSharingCommitmentPointer: validators.verifiableSecretSharingCommitmentPointer,
            list: validators.list,
            useFromTimestamp: validators.useFromTimestamp
        });
    }

    /**
     * @dev Converts `Gear.ValidationSettings` storage to `Gear.ValidationSettingsView` struct.
     * @param settings The `Gear.ValidationSettings` storage to convert.
     * @return settingsView `Gear.ValidationSettingsView` struct with the same data as the input storage.
     */
    function toView(Gear.ValidationSettings storage settings)
        internal
        view
        returns (Gear.ValidationSettingsView memory settingsView)
    {
        Gear.ValidatorsView memory validators0 = toView(settings.validators0);
        Gear.ValidatorsView memory validators1 = toView(settings.validators1);
        return Gear.ValidationSettingsView({
            thresholdNumerator: settings.thresholdNumerator,
            thresholdDenominator: settings.thresholdDenominator,
            validators0: validators0,
            validators1: validators1
        });
    }
}
