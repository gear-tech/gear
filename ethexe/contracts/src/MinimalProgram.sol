// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Proxy} from "@openzeppelin/contracts/proxy/Proxy.sol";
import {IMinimalProgram} from "./IMinimalProgram.sol";
import {IRouter} from "./IRouter.sol";

contract MinimalProgram is IMinimalProgram, Proxy {
    address public immutable router;

    constructor(address _router) {
        router = _router;
    }

    function _implementation() internal view virtual override returns (address) {
        return IRouter(router).program();
    }
}
