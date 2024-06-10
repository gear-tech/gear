// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Script, console} from "forge-std/Script.sol";
import {Router} from "../src/Router.sol";

contract RouterScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.broadcast(privateKey);
        console.log(address(new Router(vm.addr(privateKey))));
    }
}
