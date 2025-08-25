// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Script} from "forge-std/Script.sol";
import {Mirror} from "../../src/Mirror.sol";
import {Router} from "../../src/Router.sol";

contract MirrorScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        Mirror mirror = new Mirror(routerAddress);
        Router(routerAddress).setMirror(address(mirror));

        vm.stopBroadcast();
    }
}
