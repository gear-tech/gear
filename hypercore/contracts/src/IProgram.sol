// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

interface IProgram {
    function OWNER() external view returns (address);
    function stateHash() external view returns (bytes32);

    function sendMessage(bytes calldata payload, uint64 gasLimit) external payable;

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable;

    function claimValue(bytes32 messageId) external;

    function performStateTransition(bytes32 oldStateHash, bytes32 newStateHash) external;
}
