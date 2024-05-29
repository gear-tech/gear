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

    function test_X() public {
        bytes32[] memory codeIdsArray = new bytes32[](1);
        codeIdsArray[0] = bytes32(uint256(41));

        Router.CreateProgramData[] memory createProgramsArray = new Router.CreateProgramData[](1);
        createProgramsArray[0].salt = bytes32(uint256(40));
        createProgramsArray[0].codeId = bytes32(uint256(41));
        createProgramsArray[0].stateHash = bytes32(uint256(42));

        Router.UpdateProgramData[] memory updateProgramsArray = new Router.UpdateProgramData[](0);

        Router.CommitData memory commitData = Router.CommitData(codeIdsArray, createProgramsArray, updateProgramsArray);
        router.commit(commitData);
    }
}
