// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.33;

import {Script} from "forge-std/Script.sol";
import {MirrorProxy} from "src/MirrorProxy.sol";

contract MirrorProxyScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(privateKey);

        new MirrorProxy();

        vm.stopBroadcast();
    }
}
