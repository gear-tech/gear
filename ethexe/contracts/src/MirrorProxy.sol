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

    constructor(address _router) {
        router = _router;
    }

    function _implementation() internal view virtual override returns (address) {
        return IRouter(router).mirror();
    }
}
