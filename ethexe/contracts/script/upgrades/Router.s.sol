// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script, console} from "forge-std/Script.sol";
import {Router} from "../../src/Router.sol";

contract RouterScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        bool reinitialize = vm.envBool("REINITIALIZE");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        bytes memory data =
            reinitialize ? abi.encodeCall(Router.reinitialize, () /*Router.reinitialize arguments*/ ) : new bytes(0);
        Upgrades.upgradeProxy(routerAddress, "Router.sol", data);

        vm.stopBroadcast();
    }
}
