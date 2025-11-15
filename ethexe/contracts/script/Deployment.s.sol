// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Mirror} from "../src/Mirror.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Router} from "../src/Router.sol";
import {Script, console} from "forge-std/Script.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

import {Middleware} from "../src/Middleware.sol";
import {IMiddleware} from "../src/IMiddleware.sol";
import {
    IDefaultOperatorRewardsFactory
} from "symbiotic-rewards/src/interfaces/defaultOperatorRewards/IDefaultOperatorRewardsFactory.sol";

contract DeploymentScript is Script {
    WrappedVara public wrappedVara;
    Router public router;
    Mirror public mirror;
    Middleware public middleware;

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
        address middlewareAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);

        router = Router(
            payable(Upgrades.deployTransparentProxy(
                    "Router.sol",
                    deployerAddress,
                    abi.encodeCall(
                        Router.initialize,
                        (
                            deployerAddress,
                            mirrorAddress,
                            address(wrappedVara),
                            middlewareAddress,
                            1 days,
                            2 hours,
                            5 minutes,
                            Gear.AggregatedPublicKey(aggregatedPublicKeyX, aggregatedPublicKeyY),
                            verifiableSecretSharingCommitment,
                            validatorsArray
                        )
                    )
                ))
        );

        mirror = new Mirror(address(router));

        // Don't deploy middleware in dev mode
        if (!(vm.envExists("DEV_MODE") && vm.envBool("DEV_MODE"))) {
            address operatorRewardsFactoryAddress = vm.envAddress("SYMBIOTIC_OPERATOR_REWARDS_FACTORY");

            Gear.SymbioticContracts memory symbiotic = Gear.SymbioticContracts({
                vaultRegistry: vm.envAddress("SYMBIOTIC_VAULT_REGISTRY"),
                operatorRegistry: vm.envAddress("SYMBIOTIC_OPERATOR_REGISTRY"),
                networkRegistry: vm.envAddress("SYMBIOTIC_NETWORK_REGISTRY"),
                middlewareService: vm.envAddress("SYMBIOTIC_MIDDLEWARE_SERVICE"),
                networkOptIn: vm.envAddress("SYMBIOTIC_NETWORK_OPT_IN"),
                stakerRewardsFactory: vm.envAddress("SYMBIOTIC_STAKER_REWARDS_FACTORY"),
                operatorRewards: IDefaultOperatorRewardsFactory(operatorRewardsFactoryAddress).create(),
                roleSlashRequester: address(router),
                roleSlashExecutor: address(router),
                vetoResolver: address(router)
            });

            IMiddleware.InitParams memory initParams = IMiddleware.InitParams({
                owner: deployerAddress,
                eraDuration: 1 days,
                minVaultEpochDuration: 2 hours,
                operatorGracePeriod: 5 minutes,
                vaultGracePeriod: 5 minutes,
                minVetoDuration: 2 hours,
                minSlashExecutionDelay: 5 minutes,
                allowedVaultImplVersion: 1,
                vetoSlasherImplType: 1,
                maxResolverSetEpochsDelay: 5 minutes,
                collateral: address(wrappedVara),
                maxAdminFee: 10000,
                router: address(router),
                symbiotic: symbiotic
            });

            middleware = Middleware(
                Upgrades.deployTransparentProxy(
                    "Middleware.sol", deployerAddress, abi.encodeCall(Middleware.initialize, (initParams))
                )
            );

            vm.assertEq(middlewareAddress, address(middleware));
        }

        wrappedVara.approve(address(router), type(uint256).max);

        if (vm.envExists("SENDER_ADDRESS")) {
            address senderAddress = vm.envAddress("SENDER_ADDRESS");
            bool success = wrappedVara.transfer(senderAddress, 500_000 * (10 ** wrappedVara.decimals()));
            vm.assertTrue(success);
        }

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
        view
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
            console.log("curl \\");
            console.log(string.concat("    --data \"address=", vm.toString(contractAddress), "\" \\"));
            console.log(
                string.concat("    --data \"expectedimplementation=", vm.toString(expectedImplementation), "\" \\")
            );
            console.log(
                string.concat(
                    "    \"https://api.etherscan.io/v2/api?chainid=",
                    vm.toString(chainId),
                    "&module=contract&action=verifyproxycontract&apikey=$ETHERSCAN_API_KEY\""
                )
            );
            console.log("```");
        }
        console.log("================================================================================================");
    }
}
