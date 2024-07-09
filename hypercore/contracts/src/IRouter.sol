// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

interface IRouter {
    enum CodeState {
        Unknown,
        Unconfirmed,
        Confirmed
    }

    struct CodeCommitment {
        bytes32 codeId;
        bool approved;
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

    struct StateTransition {
        address actorId;
        bytes32 oldStateHash;
        bytes32 newStateHash;
        OutgoingMessage[] outgoingMessages;
    }

    struct BlockCommitment {
        bytes32 blockHash;
        bytes32 allowedPrevCommitmentHash;
        bytes32 allowedPredBlockHash;
        StateTransition[] transitions;
    }

    event BlockCommitted(bytes32 indexed blockHash);

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

    function COUNT_OF_VALIDATORS() external view returns (uint256);
    function REQUIRED_SIGNATURES() external view returns (uint256);

    function program() external view returns (address);
    function wrappedVara() external view returns (address);
    function countOfValidators() external view returns (uint256);
    // TODO: support mappings: validators, codeIds, programs

    function addValidators(address[] calldata validatorsArray) external;

    function removeValidators(address[] calldata validatorsArray) external;

    function uploadCode(bytes32 codeId, bytes32 blobTx) external;

    function createProgram(bytes32 codeId, bytes32 salt, bytes calldata initPayload, uint64 gasLimit)
        external
        payable
        returns (address);

    function sendMessage(address destination, bytes calldata payload, uint64 gasLimit, uint128 value) external;

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit, uint128 value) external;

    function claimValue(bytes32 messageId) external;

    function commitCodes(CodeCommitment[] calldata codeCommitmentsArray, bytes[] calldata signatures) external;
    function commitBlocks(BlockCommitment[] calldata blockCommitmentsArray, bytes[] calldata signatures) external;
}
