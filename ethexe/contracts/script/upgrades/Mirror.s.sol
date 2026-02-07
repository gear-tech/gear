// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Mirror} from "../../src/Mirror.sol";
import {Router} from "../../src/Router.sol";
import {Script} from "forge-std/Script.sol";

contract MirrorScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        Mirror mirror = new Mirror(payable(routerAddress));
        Router(payable(routerAddress)).setMirror(address(mirror));

        vm.stopBroadcast();
    }
}
