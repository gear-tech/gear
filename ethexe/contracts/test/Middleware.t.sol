// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Middleware} from "../src/Middleware.sol";
import {Test, console} from "forge-std/Test.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";

contract MiddlewareTest is Test {
    using MessageHashUtils for address;

    Middleware public middleware;

    function setUp() public {
        middleware = new Middleware(100, address(0), address(0), address(0), address(0), address(0), address(0));
    }

    function test_constructor() public view {
        console.log("ERA_DURATION: ", uint256(middleware.ERA_DURATION()));
        assertEq(uint256(middleware.ERA_DURATION()), 100);
        assertEq(uint256(middleware.GENESIS_TIMESTAMP()), Time.timestamp());
        assertEq(middleware.DELEGATOR_FACTORY(), address(0));
        assertEq(middleware.OPERATOR_SPECIFIC_DELEGATOR_TYPE_INDEX(), address(0));
        assertEq(middleware.SLASHER_FACTORY(), address(0));
        assertEq(middleware.OPERATOR_REGISTRY(), address(0));
        assertEq(middleware.NETWORK_REGISTRY(), address(0));
        assertEq(middleware.COLLATERAL(), address(0));
    }
}
