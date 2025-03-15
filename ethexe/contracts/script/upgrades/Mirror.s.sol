// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {MirrorImpl} from "../../src/MirrorImpl.sol";
import {Router} from "../../src/Router.sol";

contract MirrorScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        MirrorImpl mirror = new MirrorImpl();
        Router(routerAddress).setMirrorImpl(address(mirror));

        vm.stopBroadcast();
    }
}
