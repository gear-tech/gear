// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script} from "forge-std/Script.sol";
import {WrappedVara} from "../../src/WrappedVara.sol";

contract WrappedVaraScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        bool reinitialize = vm.envBool("REINITIALIZE");
        address wrappedVaraAddress = vm.envAddress("WRAPPED_VARA_ADDRESS");

        vm.startBroadcast(privateKey);

        bytes memory data = reinitialize
            ? abi.encodeCall(
                WrappedVara.reinitialize,
                () /*WrappedVara.reinitialize arguments*/
            )
            : new bytes(0);
        Upgrades.upgradeProxy(wrappedVaraAddress, "WrappedVara.sol", data);

        vm.stopBroadcast();
    }
}
