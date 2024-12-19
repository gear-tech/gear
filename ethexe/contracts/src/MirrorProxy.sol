// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Proxy} from "@openzeppelin/contracts/proxy/Proxy.sol";
import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IRouter} from "./IRouter.sol";

/*

    DO NOT CHANGE THIS CONTRACT.

*/

contract MirrorProxy is IMirrorProxy, Proxy {
    event StateChanged(bytes32 stateHash);
    event MessageQueueingRequested(bytes32 id, address indexed source, bytes payload, uint128 value);
    event ReplyQueueingRequested(bytes32 repliedTo, address indexed source, bytes payload, uint128 value);
    event ValueClaimingRequested(bytes32 claimedId, address indexed source);
    event ExecutableBalanceTopUpRequested(uint128 value);
    event Message(bytes32 id, address indexed destination, bytes payload, uint128 value);
    event Reply(bytes payload, uint128 value, bytes32 replyTo, bytes4 indexed replyCode);
    event ValueClaimed(bytes32 claimedId, uint128 value);

    address public immutable router;

    address public decoder;
    address public inheritor;
    address public initializer;
    bytes32 public stateHash;
    uint256 public nonce;

    constructor(address _router) {
        router = _router;
    }

    /* Primary Gear logic */

    function sendMessage(bytes calldata payload, uint128 value) external returns (bytes32) {
        _delegate();
    }

    function sendReply(bytes32 repliedTo, bytes calldata payload, uint128 value) external {
        _delegate();
    }

    function claimValue(bytes32 claimedId) external {
        _delegate();
    }

    function executableBalanceTopUp(uint128 value) external {
        _delegate();
    }

    function transferLockedValueToInheritor() external {
        _delegate();
    }

    /* MirrorProxy implementation */

    function _delegate() internal {
        _delegate(_implementation());
    }

    function _implementation() internal view virtual override returns (address) {
        return IRouter(router).mirrorImpl();
    }
}
