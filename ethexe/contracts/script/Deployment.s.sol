// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Mirror} from "../src/Mirror.sol";
import {MirrorProxy} from "../src/MirrorProxy.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Router} from "../src/Router.sol";
import {Script, console} from "forge-std/Script.sol";
import {Strings} from "@openzeppelin/contracts/utils/Strings.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract DeploymentScript is Script {
    using Strings for uint160;

    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;
    MirrorProxy public mirrorProxy;

    function setUp() public {}

    function run() public {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address[] memory validatorsArray = vm.envAddress("ROUTER_VALIDATORS_LIST", ",");
        uint256 aggregatedPublicKeyX = vm.envUint("ROUTER_AGGREGATED_PUBLIC_KEY_X");
        uint256 aggregatedPublicKeyY = vm.envUint("ROUTER_AGGREGATED_PUBLIC_KEY_Y");
        bytes memory verifiableSecretSharingCommitment = vm.envBytes("ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT");
        address deployerAddress = vm.addr(privateKey);

        vm.startBroadcast(privateKey);

        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", deployerAddress, abi.encodeCall(WrappedVara.initialize, (deployerAddress))
            )
        );

        address mirrorAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 2);
        address mirrorProxyAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);

        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                deployerAddress,
                abi.encodeCall(
                    Router.initialize,
                    (
                        deployerAddress,
                        mirrorAddress,
                        mirrorProxyAddress,
                        address(wrappedVara),
                        1 days,
                        2 hours,
                        5 minutes,
                        Gear.AggregatedPublicKey(aggregatedPublicKeyX, aggregatedPublicKeyY),
                        verifiableSecretSharingCommitment,
                        validatorsArray
                    )
                )
            )
        );
        mirror = new Mirror();
        mirrorProxy = new MirrorProxy(address(router));

        wrappedVara.approve(address(router), type(uint256).max);

        vm.roll(vm.getBlockNumber() + 1);
        router.lookupGenesisHash();

        vm.assertEq(router.mirrorImpl(), address(mirror));
        vm.assertEq(router.mirrorProxyImpl(), address(mirrorProxy));
        vm.assertEq(mirrorProxy.router(), address(router));
        vm.assertNotEq(router.genesisBlockHash(), bytes32(0));

        vm.stopBroadcast();

        printContractInfo("Router", address(router), Upgrades.getImplementationAddress(address(router)));
        printContractInfo("WVara", address(wrappedVara), Upgrades.getImplementationAddress(address(wrappedVara)));
        printContractInfo("Mirror", mirrorProxyAddress, mirrorAddress);
    }

    function printContractInfo(string memory contractName, address contractAddress, address expectedImplementation)
        public
        pure
    {
        console.log("================================================================================================");
        console.log("[ CONTRACT  ]", contractName);
        console.log("[ ADDRESS   ]", contractAddress);
        console.log("[ IMPL ADDR ]", expectedImplementation);
        console.log(
            "[ PROXY VERIFICATION ] Click \"Is this a proxy?\" on Etherscan to be able read and write as proxy."
        );
        console.log("                       Alternatively, run the following curl request.");
        console.log("```");
        console.log("curl --request POST 'https://api-holesky.etherscan.io/api' \\");
        console.log("   --header 'Content-Type: application/x-www-form-urlencoded' \\");
        console.log("   --data-urlencode 'module=contract' \\");
        console.log("   --data-urlencode 'action=verifyproxycontract' \\");
        console.log(string.concat("   --data-urlencode 'address=", uint160(contractAddress).toHexString(), "' \\"));
        console.log(
            string.concat(
                "   --data-urlencode 'expectedimplementation=", uint160(expectedImplementation).toHexString(), "' \\"
            )
        );
        console.log("   --data-urlencode \"apikey=$ETHERSCAN_API_KEY\"");
        console.log("```");
        console.log("================================================================================================");
    }
}
