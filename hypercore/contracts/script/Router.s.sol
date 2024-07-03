// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Script, console} from "forge-std/Script.sol";
import {Program} from "../src/Program.sol";
import {Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract RouterScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address wrappedVaraAddress = vm.envAddress("WRAPPED_VARA_TOKEN");
        address[] memory validatorsArray = vm.envAddress("ROUTER_VALIDATORS_LIST", ",");

        address deployerAddress = vm.addr(privateKey);
        address programAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 1);

        vm.startBroadcast(privateKey);

        Router router = new Router(deployerAddress, programAddress, wrappedVaraAddress, validatorsArray);
        Program program = new Program(address(router));

        vm.assertEq(router.program(), address(program));
        vm.assertEq(program.router(), address(router));

        WrappedVara(router.wrappedVara()).approve(address(router), type(uint256).max);
    }
}
