// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Script, console} from "forge-std/Script.sol";
import {Mirror} from "../src/Mirror.sol";
import {MirrorProxy} from "../src/MirrorProxy.sol";
import {Router} from "../src/Router.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract RouterScript is Script {
    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;
    MirrorProxy public mirrorProxy;

    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address[] memory validatorsArray = vm.envAddress("ROUTER_VALIDATORS_LIST", ",");
        address deployerAddress = vm.addr(privateKey);

        vm.startBroadcast(privateKey);

        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", deployerAddress, abi.encodeCall(WrappedVara.initialize, (deployerAddress))
            )
        );

        address mirrorAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 2);
        address mirrorProxyAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);
        address wrappedVaraAddress = address(wrappedVara);

        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                deployerAddress,
                abi.encodeCall(
                    Router.initialize,
                    (deployerAddress, mirrorAddress, mirrorProxyAddress, wrappedVaraAddress, validatorsArray)
                )
            )
        );
        mirror = new Mirror();
        mirrorProxy = new MirrorProxy(address(router));

        // TODO (breathx): remove this approve.
        wrappedVara.approve(address(router), type(uint256).max);

        vm.stopBroadcast();

        vm.assertEq(router.mirror(), address(mirror));
        vm.assertEq(router.mirrorProxy(), address(mirrorProxy));
        vm.assertEq(mirrorProxy.router(), address(router));
    }
}
