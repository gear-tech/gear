// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Script, console} from "forge-std/Script.sol";
import {MirrorProxySmall} from "../src/MirrorProxySmall.sol";

contract MirrorProxySmallScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(privateKey);

        new MirrorProxySmall();

        vm.stopBroadcast();
    }
}
