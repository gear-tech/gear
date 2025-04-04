// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Mirror} from "../src/Mirror.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Router} from "../src/Router.sol";
import {Script, console} from "forge-std/Script.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

contract DeploymentScript is Script {
    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;

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

        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                deployerAddress,
                abi.encodeCall(
                    Router.initialize,
                    (
                        deployerAddress,
                        mirrorAddress,
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
        mirror = new Mirror(address(router));

        wrappedVara.approve(address(router), type(uint256).max);
        wrappedVara.transfer(0x27e257aFbE253bd74055eBAb9A54A7e0F2b54958, 500_000 * (10 ** wrappedVara.decimals()));

        vm.roll(vm.getBlockNumber() + 1);
        router.lookupGenesisHash();

        vm.assertEq(router.mirrorImpl(), address(mirror));
        vm.assertNotEq(router.genesisBlockHash(), bytes32(0));

        vm.stopBroadcast();

        printContractInfo("Router", address(router), Upgrades.getImplementationAddress(address(router)));
        printContractInfo("WVara", address(wrappedVara), Upgrades.getImplementationAddress(address(wrappedVara)));
        printContractInfo("Mirror", mirrorAddress, address(0));
    }

    function printContractInfo(string memory contractName, address contractAddress, address expectedImplementation)
        public
    {
        console.log("================================================================================================");
        console.log("[ CONTRACT  ]", contractName);
        console.log("[ ADDRESS   ]", contractAddress);
        if (expectedImplementation != address(0)) {
            console.log("[ IMPL ADDR ]", expectedImplementation);
            console.log(
                "[ PROXY VERIFICATION ] Click \"Is this a proxy?\" on Etherscan to be able read and write as proxy."
            );
            console.log("                       Alternatively, run the following curl request.");
            console.log("```");
            uint256 chainId = block.chainid;
            if (chainId == 1) {
                console.log("curl --request POST 'https://api.etherscan.io/api' \\");
            } else {
                // https://github.com/foundry-rs/forge-std/issues/671
                console.log(
                    string.concat(
                        "curl --request POST 'https://api-",
                        chainId == 560048 ? "hoodi" : getChain(chainId).chainAlias,
                        ".etherscan.io/api' \\"
                    )
                );
            }
            console.log("   --header 'Content-Type: application/x-www-form-urlencoded' \\");
            console.log("   --data-urlencode 'module=contract' \\");
            console.log("   --data-urlencode 'action=verifyproxycontract' \\");
            console.log(string.concat("   --data-urlencode 'address=", vm.toString(contractAddress), "' \\"));
            console.log(
                string.concat(
                    "   --data-urlencode 'expectedimplementation=", vm.toString(expectedImplementation), "' \\"
                )
            );
            console.log("   --data-urlencode \"apikey=$ETHERSCAN_API_KEY\"");
            console.log("```");
        }
        console.log("================================================================================================");
    }
}
