// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {Script} from "forge-std/Script.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {POAMiddleware} from "src/POAMiddleware.sol";

contract POAMiddlewareScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        bool reinitialize = vm.envBool("REINITIALIZE");
        address poaMiddlewareAddress = vm.envAddress("POA_MIDDLEWARE_ADDRESS");

        vm.startBroadcast(privateKey);

        bytes memory data = reinitialize
            ? abi.encodeCall(
                POAMiddleware.reinitialize,
                () /*POAMiddleware.reinitialize arguments*/
            )
            : new bytes(0);
        Upgrades.upgradeProxy(poaMiddlewareAddress, "POAMiddleware.sol", data);

        vm.stopBroadcast();
    }
}
