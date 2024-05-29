// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Script, console} from "forge-std/Script.sol";
import {Program} from "../src/Program.sol";

contract ProgramScript is Script {
    function setUp() public {}

    function run() public {
        vm.broadcast(vm.envUint("PRIVATE_KEY"));
        console.log(address(new Program()));
    }
}
