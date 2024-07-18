// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IMinimalProgram} from "./IMinimalProgram.sol";
import {IProgram} from "./IProgram.sol";
import {IRouter} from "./IRouter.sol";

contract Program is IProgram {
    bytes32 public stateHash;

    function router() public view returns (address) {
        return IMinimalProgram(address(this)).router();
    }

    function sendMessage(bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(router()).sendMessage(address(this), payload, gasLimit, uint128(msg.value));
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit) external payable {
        IRouter(router()).sendReply(replyToId, payload, gasLimit, uint128(msg.value));
    }

    function claimValue(bytes32 messageId) external {
        IRouter(router()).claimValue(messageId);
    }

    modifier onlyRouter() {
        require(msg.sender == router(), "not router");
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
