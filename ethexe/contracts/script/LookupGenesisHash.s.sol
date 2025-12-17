// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Script} from "forge-std/Script.sol";
import {IRouter} from "../src/IRouter.sol";

contract DummyContract {}

contract LookupGenesisHashScript is Script {
    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address routerAddress = vm.envAddress("ROUTER_ADDRESS");
        vm.startBroadcast(privateKey);

        new DummyContract(); // Hack to send transaction in new block with `--slow` flag

        IRouter router = IRouter(routerAddress);

        vm.roll(vm.getBlockNumber() + 1);
        router.lookupGenesisHash();
        vm.assertNotEq(router.genesisBlockHash(), bytes32(0));

        vm.stopBroadcast();
    }
}
