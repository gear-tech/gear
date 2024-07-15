// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script, console} from "forge-std/Script.sol";
import {Program} from "../src/Program.sol";
import {MinimalProgram} from "../src/MinimalProgram.sol";
import {Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract RouterScript is Script {
    WrappedVara public wrappedVara;
    Router public router;
    Program public program;
    MinimalProgram public minimalProgram;

    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address[] memory validatorsArray = vm.envAddress("ROUTER_VALIDATORS_LIST", ",");
        address deployerAddress = vm.addr(privateKey);

        vm.startBroadcast(privateKey);

        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", deployerAddress, abi.encodeCall(WrappedVara.initialize, (deployerAddress, 6))
            )
        );

        address programAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 2);
        address minimalProgramAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);
        address wrappedVaraAddress = address(wrappedVara);

        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                deployerAddress,
                abi.encodeCall(
                    Router.initialize,
                    (deployerAddress, programAddress, minimalProgramAddress, wrappedVaraAddress, validatorsArray)
                )
            )
        );
        program = new Program();
        minimalProgram = new MinimalProgram(address(router));

        wrappedVara.approve(address(router), type(uint256).max);

        vm.stopBroadcast();

        vm.assertEq(router.program(), address(program));
        vm.assertEq(router.minimalProgram(), address(minimalProgram));
        vm.assertEq(minimalProgram.router(), address(router));
    }
}
