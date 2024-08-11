// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

interface IProgram {
    function router() external view returns (address);
    function stateHash() external view returns (bytes32);

    function sendMessage(bytes calldata payload, uint64 gasLimit) external payable;

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable;

    function claimValue(bytes32 messageId) external;

    function performStateTransition(bytes32 oldStateHash, bytes32 newStateHash) external;

    function performPayout(address actorId, uint128 value) external;
}
