// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {IWrappedVara} from "./IWrappedVara.sol";

contract Program {
    address public constant OWNER = 0x2e234DAe75C793f67A35089C9d99245E1C58470b;
    address public constant WRAPPED_VARA = 0x6377Bf194281FF2b14e807CC3740ac937744406f;

    bytes32 public stateHash;

    event SendMessage(address origin, address destination, bytes payload, uint64 gasLimit, uint128 value);

    event SendReply(address origin, bytes32 replyToId, bytes payload, uint64 gasLimit, uint128 value);

    event ClaimValue(address origin, bytes32 messageId);

    function sendMessage(address destination, bytes calldata payload, uint64 gasLimit) external payable {
        IWrappedVara wrappedVara = IWrappedVara(WRAPPED_VARA);
        bool success = wrappedVara.transferFrom(msg.sender, OWNER, wrappedVara.gasToValue(gasLimit));
        require(success, "failed to transfer tokens");
        emit SendMessage(tx.origin, destination, payload, gasLimit, uint128(msg.value));
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable {
        IWrappedVara wrappedVara = IWrappedVara(WRAPPED_VARA);
        bool success = wrappedVara.transferFrom(msg.sender, OWNER, wrappedVara.gasToValue(gasLimit));
        require(success, "failed to transfer tokens");
        emit SendReply(tx.origin, replyToId, payload, gasLimit, uint128(msg.value));
    }

    function claimValue(bytes32 messageId) external {
        emit ClaimValue(tx.origin, messageId);
    }

    function performStateTransition(bytes32 oldStateHash, bytes32 newStateHash) external {
        require(msg.sender == OWNER, "not owner");
        require(stateHash == oldStateHash, "invalid state transition");
        stateHash = newStateHash;
    }
}
