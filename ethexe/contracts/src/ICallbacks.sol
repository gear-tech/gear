// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

interface ICallbacks {
    function onErrorReply(bytes32 messageId, bytes calldata payload, bytes4 replyCode) external payable;
}
