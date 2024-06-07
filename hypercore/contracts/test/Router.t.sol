// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {Test, console} from "forge-std/Test.sol";
import {Program} from "../src/Program.sol";
import {Router} from "../src/Router.sol";

contract RouterTest is Test {
    Program public program;
    Router public router;

    function setUp() public {
        program = new Program();
        router = new Router();
        router.setProgram(address(program));
    }
}
