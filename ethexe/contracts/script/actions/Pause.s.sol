// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.33;

import {Script} from "forge-std/Script.sol";
import {Router} from "src/Router.sol";

contract PauseScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        Router(payable(routerAddress)).pause();

        vm.stopBroadcast();
    }
}
