// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

interface ICallbacks {
    function errorReply(bytes32 messageId, bytes4 replyCode) external;
}
