// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Script, console} from "forge-std/Script.sol";
import {Router} from "../src/Router.sol";

contract RouterScript is Script {
    function setUp() public {}

    function run() public {
        vm.broadcast(vm.envUint("PRIVATE_KEY"));
        console.log(address(new Router()));
    }
}
