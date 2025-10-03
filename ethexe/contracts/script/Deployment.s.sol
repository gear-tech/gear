// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.28;

import {Script} from "forge-std/Script.sol";
import {BaseDeployment} from "./BaseDeployment.sol";

contract Deployment is Script, BaseDeployment {
    function setUp() public {}

    function run() public {
        deployGearExeFromEnvironment();
    }
}
