// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

// TODO (breathx): sort here everything.
interface IRouter {
    /* Storage related structures */

    /// @custom:storage-location erc7201:router.storage.Router
    struct Storage {
        bytes32 genesisBlockHash;
        address mirror;
        address mirrorProxy;
        address wrappedVara;
        bytes32 lastBlockCommitmentHash;
        uint256 signingThresholdPercentage;
        uint64 baseWeight;
        uint128 valuePerWeight;
        mapping(address => bool) validators;
        address[] validatorsKeys;
        mapping(bytes32 => CodeState) codes;
        uint256 validatedCodesCount;
        mapping(address => bool) programs;
        uint256 programsCount;
    }

    enum CodeState {
        Unknown,
        ValidationRequested,
        Validated
    }

    /* Commitment related structures */

    struct CodeCommitment {
        bytes32 id;
        bool valid;
    }

    struct BlockCommitment {
        bytes32 blockHash;
        bytes32 prevCommitmentHash;
        bytes32 predBlockHash;
        StateTransition[] transitions;
    }

    struct StateTransition {
        address actorId;
        bytes32 newStateHash;
        uint128 valueToReceive;
        ValueClaim[] valueClaims;
        OutgoingMessage[] messages;
    }

    struct ValueClaim {
        bytes32 messageId;
        address destination;
        uint128 value;
    }

    struct OutgoingMessage {
        bytes32 id;
        address destination;
        bytes payload;
        uint128 value;
        ReplyDetails replyDetails;
    }

    struct ReplyDetails {
        bytes32 to;
        bytes4 code;
    }

    /* Events section */

    /**
     * @dev Emitted when a new state transitions are applied.
     */
    event BlockCommitted(bytes32 blockHash);

    /**
     * @dev Emitted when a new code validation request submitted.
     */
    event CodeValidationRequested(bytes32 codeId, bytes32 blobTxHash);

    /**
     * @dev Emitted when a code, previously requested to be validated, gets validated.
     */
    event CodeGotValidated(bytes32 id, bool indexed valid);

    // TODO: consider splitting init message creation.
    /**
     * @dev Emitted when a new program created.
     */
    event ProgramCreated(address actorId, bytes32 indexed codeId);

    /**
     * @dev Emitted when the validators set is changed.
     */
    event ValidatorsSetChanged();

    /**
     * @dev Emitted when the storage slot is changed.
     */
    event StorageSlotChanged();

    /**
     * @dev Emitted when the tx's base weight is changed.
     */
    event BaseWeightChanged(uint64 baseWeight);

    /**
     * @dev Emitted when the value per executable weight is changed.
     */
    event ValuePerWeightChanged(uint128 valuePerWeight);

    /* Functions section */

    /* Operational functions */

    function getStorageSlot() external view returns (bytes32);

    function setStorageSlot(string calldata namespace) external;

    function genesisBlockHash() external view returns (bytes32);

    function lastBlockCommitmentHash() external view returns (bytes32);

    function wrappedVara() external view returns (address);

    function mirrorProxy() external view returns (address);

    function mirror() external view returns (address);

    function setMirror(address mirror) external;

    /* Codes and programs observing functions */

    function validatedCodesCount() external view returns (uint256);

    function codeState(bytes32 codeId) external view returns (CodeState);

    function programsCount() external view returns (uint256);

    function programExists(address program) external view returns (bool);

    /* Validators' set related functions */

    function signingThresholdPercentage() external view returns (uint256);

    function validatorsThreshold() external view returns (uint256);

    function validatorsCount() external view returns (uint256);

    function validatorExists(address validator) external view returns (bool);

    function validators() external view returns (address[] memory);

    function updateValidators(address[] calldata validatorsAddressArray) external;

    /* Economic and token related functions */

    function baseWeight() external view returns (uint64);

    function setBaseWeight(uint64 baseWeight) external;

    function valuePerWeight() external view returns (uint128);

    function setValuePerWeight(uint128 valuePerWeight) external;

    function baseFee() external view returns (uint128);

    /* Primary Gear logic */

    function requestCodeValidation(bytes32 codeId, bytes32 blobTxHash) external;

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata payload, uint128 value)
        external
        payable
        returns (address);

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external;

    function commitBlocks(BlockCommitment[] calldata blockCommitmentsArray, bytes[] calldata signatures) external;
}
