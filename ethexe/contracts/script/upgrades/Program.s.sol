// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Script, console} from "forge-std/Script.sol";
import {Program} from "../../src/Program.sol";
import {Router} from "../../src/Router.sol";

contract ProgramScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");

        vm.startBroadcast(privateKey);

        Program program = new Program();
        Router(routerAddress).setProgram(address(program));

        vm.stopBroadcast();
    }
}
