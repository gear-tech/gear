// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script, console} from "forge-std/Script.sol";
import {WrappedVara} from "../../src/WrappedVara.sol";

contract WrappedVaraScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address wrappedVaraAddress = vm.envAddress("WRAPPED_VARA_ADDRESS");

        vm.startBroadcast(privateKey);

        // How to do state change upgrades:
        // 1. Uncomment WrappedVara.reinitialize in WrappedVara.sol.
        // 2. Use the following code:
        // Upgrades.upgradeProxy(
        //     wrappedVaraAddress,
        //     "WrappedVara.sol",
        //     abi.encodeCall(WrappedVara.reinitialize, () /*WrappedVara.reinitialize arguments*/ )
        // );

        // How to do business logic upgrades:
        // Upgrades.upgradeProxy(wrappedVaraAddress, "WrappedVara.sol", "");

        vm.stopBroadcast();
    }
}
