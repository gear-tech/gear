// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

contract Program {
    address public constant OWNER = 0x2e234DAe75C793f67A35089C9d99245E1C58470b;
    bytes32 public stateHash;

    event SendMessage(address origin, address destination, bytes payload, uint64 gasLimit, uint128 value);

    event SendReply(address origin, bytes32 replyToId, bytes payload, uint64 gasLimit, uint128 value);

    event ClaimValue(address origin, bytes32 messageId);

    function sendMessage(address destination, bytes calldata payload, uint64 gasLimit, uint128 value)
        external
        payable
    {
        emit SendMessage(tx.origin, destination, payload, gasLimit, value);
    }

    function sendReply(bytes32 replyToId, bytes calldata payload, uint64 gasLimit, uint128 value) external payable {
        emit SendReply(tx.origin, replyToId, payload, gasLimit, value);
    }

    function claimValue(bytes32 messageId) external {
        emit ClaimValue(tx.origin, messageId);
    }

    function setStateHash(bytes32 _stateHash) external {
        require(msg.sender == OWNER, "not owner");
        stateHash = _stateHash;
    }
}
