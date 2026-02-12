// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {SlotDerivation} from "@openzeppelin/contracts/utils/SlotDerivation.sol";
import {TransientSlot} from "@openzeppelin/contracts/utils/TransientSlot.sol";
import {ECDSA} from "@openzeppelin/contracts/utils/cryptography/ECDSA.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {FROST} from "frost-secp256k1-evm/FROST.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";
import {IRouter} from "src/IRouter.sol";

library Gear {
    using ECDSA for bytes32;
    using MessageHashUtils for address;

    using TransientSlot for *;
    using SlotDerivation for *;

    // 2.5 * 10^9 of gear gas.
    uint64 public constant COMPUTATION_THRESHOLD = 2_500_000_000;

    // 2/3; validators signatures to verify.
    uint128 public constant VALIDATORS_THRESHOLD_NUMERATOR = 2;
    uint128 public constant VALIDATORS_THRESHOLD_DENOMINATOR = 3;

    // 10 WVara tokens per compute second.
    uint128 public constant WVARA_PER_SECOND = 10_000_000_000_000;

    error ValidationBeforeGenesis();

    error TimestampOlderThanPreviousEra();

    error TimestampInFuture();

    error InvalidFrostSignatureCount();

    error InvalidFrostSignatureLength();

    error ErasTimestampMustNotBeEqual();

    error ValidatorsNotFoundForTimestamp();

    uint256 internal constant COMMIT_BATCH_AFTER_ERA_STARTED_AT = 73;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_IS_PREVIOUS_ERA_VALIDATION = 74;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_IS_PREVIOUS_ERA_VALIDATION = 75;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_VALIDATION_BEFORE_GENESIS = 76;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_VALIDATION_BEFORE_GENESIS = 77;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_TIMESTAMP_OLDER_THAN_PREVIOUS_ERA = 78;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_TIMESTAMP_OLDER_THAN_PREVIOUS_ERA = 79;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_TIMESTAMP_IN_FUTURE = 80;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_TIMESTAMP_IN_FUTURE = 81;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_TS_LESS_THAN_ERA_STARTED = 82;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_TS_LESS_THAN_ERA_STARTED = 83;

    uint256 internal constant COMMIT_BATCH_BEFORE_CALLING_VALIDATORS_AT = 84;
    uint256 internal constant COMMIT_BATCH_AFTER_CALLING_VALIDATORS_AT = 85;

    uint256 internal constant COMMIT_BATCH_BEFORE_CALCULATING_MESSAGE_HASH = 86;
    uint256 internal constant COMMIT_BATCH_AFTER_CALCULATING_MESSAGE_HASH = 87;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_SIGNATURE_TYPE = 88;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_SIGNATURE_TYPE = 89;

    uint256 internal constant COMMIT_BATCH_BEFORE_CALCULATING_VALIDATORS_THRESHOLD = 90;
    uint256 internal constant COMMIT_BATCH_AFTER_CALCULATING_VALIDATORS_THRESHOLD = 91;

    uint256 internal constant COMMIT_BATCH_BEFORE_SETTING_VALID_SIGNATURES_TO_ZERO = 92;
    uint256 internal constant COMMIT_BATCH_AFTER_SETTING_VALID_SIGNATURES_TO_ZERO = 93;

    uint256 internal constant COMMIT_BATCH_BEFORE_TAKING_SIGNATURE = 94;
    uint256 internal constant COMMIT_BATCH_AFTER_TAKING_SIGNATURE = 95;

    uint256 internal constant COMMIT_BATCH_BEFORE_RECOVERING_VALIDATOR_ADDRESS = 96;
    uint256 internal constant COMMIT_BATCH_AFTER_RECOVERING_VALIDATOR_ADDRESS = 97;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_VALIDATOR_IN_MAP = 98;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_VALIDATOR_IN_MAP = 99;

    uint256 internal constant COMMIT_BATCH_BEFORE_LOADING_TRANSIENT_STORAGE_SLOT = 100;
    uint256 internal constant COMMIT_BATCH_AFTER_LOADING_TRANSIENT_STORAGE_SLOT = 101;

    uint256 internal constant COMMIT_BATCH_BEFORE_STORING_IN_TRANSIENT_STORAGE_SLOT = 102;
    uint256 internal constant COMMIT_BATCH_AFTER_STORING_IN_TRANSIENT_STORAGE_SLOT = 103;

    uint256 internal constant COMMIT_BATCH_BEFORE_CHECKING_SIGNATURES_COUNT = 104;
    uint256 internal constant COMMIT_BATCH_AFTER_CHECKING_SIGNATURES_COUNT = 105;

    uint256 internal constant COMMIT_BATCH_BEFORE_EXITING_VERIFY_AT_FUNCTION = 106;

    event DebugEvent(uint256 indexed topic0) anonymous;

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

    struct ValidatorsView {
        AggregatedPublicKey aggregatedPublicKey;
        address verifiableSecretSharingCommitmentPointer;
        address[] list;
        uint256 useFromTimestamp;
    }

    struct AddressBook {
        address mirror;
        address wrappedVara;
        address middleware;
    }

    struct CodeCommitment {
        bytes32 id;
        bool valid;
    }

    struct ChainCommitment {
        /// @dev Transitions of program states, value and messages.
        StateTransition[] transitions;
        /// @dev Head of chain. Hash of the last block in chain.
        bytes32 head;
    }

    struct ValidatorsCommitment {
        AggregatedPublicKey aggregatedPublicKey;
        bytes verifiableSecretSharingCommitment;
        address[] validators;
        uint256 eraIndex;
    }

    struct BatchCommitment {
        /// @dev Hash of ethereum block for which the batch was created.
        bytes32 blockHash;
        /// @dev Timestamp of ethereum block for which this batch was created.
        uint48 blockTimestamp;
        /// @dev Hash of previously committed batch hash.
        bytes32 previousCommittedBatchHash;
        /// @dev Expiry in blocks since `blockHash`.
        /// if 1 - then valid only in child block
        /// if 2 - then valid in child and grandchild blocks
        /// ... etc.
        uint8 expiry;
        /// @dev Chain commitment (contains one or zero commitments)
        ChainCommitment[] chainCommitment;
        /// @dev Code commitments
        CodeCommitment[] codeCommitments;
        /// @dev Rewards commitment (contains one or zero commitments)
        RewardsCommitment[] rewardsCommitment;
        /// @dev Validators commitment (contains one or zero commitments)
        ValidatorsCommitment[] validatorsCommitment;
    }

    struct RewardsCommitment {
        OperatorRewardsCommitment operators;
        StakerRewardsCommitment stakers;
        uint48 timestamp;
    }

    struct OperatorRewardsCommitment {
        uint256 amount;
        bytes32 root;
    }

    struct StakerRewardsCommitment {
        StakerRewards[] distribution;
        uint256 totalAmount;
        address token;
    }

    struct StakerRewards {
        address vault;
        uint256 amount;
    }

    enum CodeState {
        Unknown,
        ValidationRequested,
        Validated
    }

    struct CommittedBatchInfo {
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
        // TODO (breathx): use ReplyDetails[]
        ReplyDetails replyDetails;
        bool call;
    }

    struct ProtocolData {
        mapping(bytes32 => CodeState) codes;
        mapping(address => bytes32) programs;
        uint256 programsCount;
        uint256 validatedCodesCount;
    }

    struct ReplyDetails {
        bytes32 to;
        // TODO (breathx): consider struct and methods to determine reason.
        // TODO (breathx): consider avoid submitting auto replies.
        bytes4 code;
    }

    struct StateTransition {
        address actorId;
        bytes32 newStateHash;
        bool exited;
        address inheritor;
        /// @dev We represent `valueToReceive` as `uint128` and `bool` because each non-zero byte costs 16 gas,
        ///      and each zero byte costs 4 gas (see https://evm.codes/about#gascosts).
        ///      Also see `ethexe/common/src/gear.rs`.
        uint128 valueToReceive;
        bool valueToReceiveNegativeSign;
        ValueClaim[] valueClaims;
        Message[] messages;
    }

    struct Timelines {
        uint256 era;
        uint256 election;
        uint256 validationDelay;
    }

    struct ValidationSettings {
        uint128 thresholdNumerator;
        uint128 thresholdDenominator;
        Validators validators0;
        Validators validators1;
    }

    struct ValidationSettingsView {
        uint128 thresholdNumerator;
        uint128 thresholdDenominator;
        ValidatorsView validators0;
        ValidatorsView validators1;
    }

    struct ValueClaim {
        bytes32 messageId;
        address destination;
        uint128 value;
    }

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

    enum SignatureType {
        FROST,
        ECDSA
    }

    function chainCommitmentHash(bytes32 _transitionsHash, bytes32 _head) internal pure returns (bytes32) {
        return Hashes.efficientKeccak256AsBytes32(_transitionsHash, _head);
    }

    function codeCommitmentHash(bytes32 codeId, bool valid) internal pure returns (bytes32) {
        bytes32 _codeCommitmentHash;
        assembly ("memory-safe") {
            mstore(0x00, codeId)
            mstore8(0x20, valid)
            _codeCommitmentHash := keccak256(0x00, 0x21)
        }
        return _codeCommitmentHash;
    }

    function rewardsCommitmentHash(bytes32 _operatorRewardsHash, bytes32 _stakerRewardsHash, uint48 _timestamp)
        internal
        pure
        returns (bytes32)
    {
        return keccak256(abi.encodePacked(_operatorRewardsHash, _stakerRewardsHash, _timestamp));
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

    function valueClaimHash(bytes32 _messageId, address _destination, uint128 _value) internal pure returns (bytes32) {
        return keccak256(abi.encodePacked(_messageId, _destination, _value));
    }

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

    function blockIsPredecessor(bytes32 hash, uint8 expiry) internal view returns (bool) {
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

    function defaultComputationSettings() internal pure returns (ComputationSettings memory) {
        return ComputationSettings({threshold: COMPUTATION_THRESHOLD, wvaraPerSecond: WVARA_PER_SECOND});
    }

    function newGenesis() internal view returns (GenesisBlockInfo memory) {
        return GenesisBlockInfo({hash: bytes32(0), number: uint32(block.number), timestamp: uint48(block.timestamp)});
    }

    /// @dev Validates signatures of the given data hash.
    function validateSignatures(
        IRouter.Storage storage router,
        bytes32 routerTransientStorage,
        bytes32 _dataHash,
        Gear.SignatureType _signatureType,
        bytes[] calldata _signatures
    ) internal returns (bool) {
        return validateSignaturesAt(
            router, routerTransientStorage, _dataHash, _signatureType, _signatures, block.timestamp
        );
    }

    /// @dev Validates signatures of the given data hash at the given timestamp.
    /// TODO: support native keyword `transient storage`: https://github.com/foundry-rs/foundry/issues/9931
    function validateSignaturesAt(
        IRouter.Storage storage router,
        bytes32 routerTransientStorage,
        bytes32 _dataHash,
        SignatureType _signatureType,
        bytes[] calldata _signatures,
        uint256 ts
    ) internal returns (bool) {
        uint256 eraStarted = eraStartedAt(router, block.timestamp);
        // gas used: ~4883 (enter to function and calculate era started at)
        // emit DebugEvent(COMMIT_BATCH_AFTER_ERA_STARTED_AT);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_IS_PREVIOUS_ERA_VALIDATION);
        if (ts < eraStarted && block.timestamp < eraStarted + router.timelines.validationDelay) {
            // TODO: recalc here everything, not sure

            // gas used: ~2993 (checking is previous era validation)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_IS_PREVIOUS_ERA_VALIDATION);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_VALIDATION_BEFORE_GENESIS);
            require(ts >= router.genesisBlock.timestamp, ValidationBeforeGenesis());
            // gas used: ~782 (checking validation before genesis)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_VALIDATION_BEFORE_GENESIS);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_TIMESTAMP_OLDER_THAN_PREVIOUS_ERA);
            require(ts + router.timelines.era >= eraStarted, TimestampOlderThanPreviousEra());
            // gas used: ~837 (checking timestamp older than previous era)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_TIMESTAMP_OLDER_THAN_PREVIOUS_ERA);

            // Validation must be done using validators from previous era,
            // because `ts` is in the past and we are in the validation delay period.
        } else {
            // gas used: ~67 (checking is previous era validation)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_IS_PREVIOUS_ERA_VALIDATION);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_TIMESTAMP_IN_FUTURE);
            require(ts <= block.timestamp, TimestampInFuture());
            // gas used: ~35 (checking timestamp in future)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_TIMESTAMP_IN_FUTURE);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_TS_LESS_THAN_ERA_STARTED);
            if (ts < eraStarted) {
                ts = eraStarted;
            }
            // gas used: ~??? (checking ts less than era started)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_TS_LESS_THAN_ERA_STARTED);

            // Validation must be done using current era validators.
        }

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CALLING_VALIDATORS_AT);
        Validators storage validators = validatorsAt(router, ts);
        // gas used: ~4435 (calling validators at)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CALLING_VALIDATORS_AT);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CALCULATING_MESSAGE_HASH);
        bytes32 _messageHash = address(this).toDataWithIntendedValidatorHash(_dataHash);
        // gas used: ~92 (calculating message hash)
        // emit DebugEvent(COMMIT_BATCH_AFTER_CALCULATING_MESSAGE_HASH);

        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_SIGNATURE_TYPE);
        if (_signatureType == SignatureType.FROST) {
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_SIGNATURE_TYPE);
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

            /*
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
            // gas used: ~94 (checking signature type)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_SIGNATURE_TYPE);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_CALCULATING_VALIDATORS_THRESHOLD);
            uint256 threshold = validatorsThreshold(
                validators.list.length,
                router.validationSettings.thresholdNumerator,
                router.validationSettings.thresholdDenominator
            );
            // gas used: ~4430 (calculating validators threshold)
            // emit DebugEvent(COMMIT_BATCH_AFTER_CALCULATING_VALIDATORS_THRESHOLD);

            // emit DebugEvent(COMMIT_BATCH_BEFORE_SETTING_VALID_SIGNATURES_TO_ZERO);
            uint256 validSignatures = 0;
            // gas used: ~13 (setting valid signatures to zero)
            // emit DebugEvent(COMMIT_BATCH_AFTER_SETTING_VALID_SIGNATURES_TO_ZERO);

            for (uint256 i = 0; i < _signatures.length; i++) {
                // emit DebugEvent(COMMIT_BATCH_BEFORE_TAKING_SIGNATURE);
                bytes calldata signature = _signatures[i];
                // gas used: ~189 (taking signature)
                // emit DebugEvent(COMMIT_BATCH_AFTER_TAKING_SIGNATURE);

                // emit DebugEvent(COMMIT_BATCH_BEFORE_RECOVERING_VALIDATOR_ADDRESS);
                address validator = _messageHash.recover(signature);
                // gas used: ~3832 (recovering validator address)
                // emit DebugEvent(COMMIT_BATCH_AFTER_RECOVERING_VALIDATOR_ADDRESS);

                // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_VALIDATOR_IN_MAP);
                if (validators.map[validator]) {
                    // gas used: ~2220 (checking validator in map)
                    // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_VALIDATOR_IN_MAP);
                    bytes32 transientStorageValidatorsSlot = routerTransientStorage.deriveMapping(validator);

                    // emit DebugEvent(COMMIT_BATCH_BEFORE_LOADING_TRANSIENT_STORAGE_SLOT);
                    if (transientStorageValidatorsSlot.asBoolean().tload()) {
                        continue;
                    } else {
                        // gas used: ~143 (loading transient storage slot)
                        // emit DebugEvent(COMMIT_BATCH_AFTER_LOADING_TRANSIENT_STORAGE_SLOT);

                        // emit DebugEvent(COMMIT_BATCH_BEFORE_STORING_IN_TRANSIENT_STORAGE_SLOT);
                        transientStorageValidatorsSlot.asBoolean().tstore(true);
                    }
                    // gas used: ~108 (storing in transient storage slot)
                    // emit DebugEvent(COMMIT_BATCH_AFTER_STORING_IN_TRANSIENT_STORAGE_SLOT);

                    // emit DebugEvent(COMMIT_BATCH_AFTER_CHECKING_SIGNATURES_COUNT);
                    if (++validSignatures == threshold) {
                        // gas used: ~65 (checking signatures count)
                        // emit DebugEvent(COMMIT_BATCH_BEFORE_CHECKING_SIGNATURES_COUNT);

                        // emit DebugEvent(COMMIT_BATCH_BEFORE_EXITING_VERIFY_AT_FUNCTION);
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

    function validatorsThreshold(uint256 validatorsAmount, uint128 thresholdNumerator, uint128 thresholdDenominator)
        internal
        pure
        returns (uint256)
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

    function eraIndexAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return (ts - router.genesisBlock.timestamp) / router.timelines.era;
    }

    function eraStartedAt(IRouter.Storage storage router, uint256 ts) internal view returns (uint256) {
        return router.genesisBlock.timestamp + eraIndexAt(router, ts) * router.timelines.era;
    }

    function toView(Gear.Validators storage validators) internal view returns (Gear.ValidatorsView memory) {
        return Gear.ValidatorsView({
            aggregatedPublicKey: validators.aggregatedPublicKey,
            verifiableSecretSharingCommitmentPointer: validators.verifiableSecretSharingCommitmentPointer,
            list: validators.list,
            useFromTimestamp: validators.useFromTimestamp
        });
    }

    function toView(Gear.ValidationSettings storage settings)
        internal
        view
        returns (Gear.ValidationSettingsView memory)
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
