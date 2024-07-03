// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {IProgram} from "./IProgram.sol";
import {IRouter} from "./IRouter.sol";

contract Program is IProgram {
    address public immutable router;
    bytes32 public stateHash;

    constructor(address _router) {
        router = _router;
    }

    function sendMessage(bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(router).sendMessage(address(this), payload, gasLimit, uint128(msg.value));
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(router).sendReply(replyToId, payload, gasLimit, uint128(msg.value));
    }

    function claimValue(bytes32 messageId) external {
        IRouter(router).claimValue(messageId);
    }

    modifier onlyRouter() {
        require(msg.sender == router, "not router");
        _;
    }

    function performStateTransition(bytes32 oldStateHash, bytes32 newStateHash) external onlyRouter {
        require(stateHash == oldStateHash, "invalid state transition");
        stateHash = newStateHash;
    }

    function performPayout(address actorId, uint128 value) external onlyRouter {
        (bool sent,) = actorId.call{value: value}("");
        require(sent, "failed to send value");
    }
}
