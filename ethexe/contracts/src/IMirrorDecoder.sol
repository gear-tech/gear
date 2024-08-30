// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

interface IMirrorDecoder {
    function initialize() external;

    function mirror() external view returns (address);

    function onMessageSent(bytes32 id, address destination, bytes calldata payload, uint128 value) external;

    function onReplySent(address destination, bytes calldata payload, uint128 value, bytes32 replyTo, bytes4 replyCode)
        external;
}
