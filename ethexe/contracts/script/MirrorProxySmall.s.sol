// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {MirrorProxySmall} from "../src/MirrorProxySmall.sol";
import {Script} from "forge-std/Script.sol";

contract MirrorProxySmallScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(privateKey);

        new MirrorProxySmall();

        vm.stopBroadcast();
    }
}
