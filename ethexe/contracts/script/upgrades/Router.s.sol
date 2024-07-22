// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script, console} from "forge-std/Script.sol";
import {Router} from "../../src/Router.sol";

contract RouterScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        // How to do state change upgrades:
        // 1. Uncomment Router.reinitialize in Router.sol.
        // 2. Use the following code:
        // Upgrades.upgradeProxy(
        //     routerAddress, "Router.sol", abi.encodeCall(Router.reinitialize, () /*Router.reinitialize arguments*/ )
        // );

        // How to do business logic upgrades:
        // Upgrades.upgradeProxy(routerAddress, "Router.sol", "");

        vm.stopBroadcast();
    }
}
