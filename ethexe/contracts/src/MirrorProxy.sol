// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Proxy} from "@openzeppelin/contracts/proxy/Proxy.sol";
import {IMirrorProxy} from "./IMirrorProxy.sol";
import {IRouter} from "./IRouter.sol";

/*

    DO NOT CHANGE THIS CONTRACT.

*/

contract MirrorProxy is IMirrorProxy, Proxy {
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

    function sendMessage(bytes calldata payload, uint128 value) external /*returns (bytes32)*/ {
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
