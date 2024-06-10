// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {IRouter} from "./IRouter.sol";

contract Program {
    address public constant OWNER = 0x2e234DAe75C793f67A35089C9d99245E1C58470b;
    bytes32 public stateHash;

    function sendMessage(bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(OWNER).sendMessage(address(this), payload, gasLimit, uint128(msg.value));
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(OWNER).sendReply(replyToId, payload, gasLimit, uint128(msg.value));
    }

    function claimValue(bytes32 messageId) external {
        IRouter(OWNER).claimValue(messageId);
    }

    function performStateTransition(bytes32 oldStateHash, bytes32 newStateHash) external {
        require(msg.sender == OWNER, "not owner");
        require(stateHash == oldStateHash, "invalid state transition");
        stateHash = newStateHash;
    }
}
