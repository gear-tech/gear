// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {StdChains} from "forge-std/StdChains.sol";
import {StdUtils} from "forge-std/StdUtils.sol";
import {CommonBase} from "forge-std/Base.sol";
import {StdCheats} from "forge-std/StdCheats.sol";
import {StdAssertions} from "forge-std/StdAssertions.sol";
import {StdInvariant} from "forge-std/StdInvariant.sol";

import {Mirror} from "../src/Mirror.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {Router} from "../src/Router.sol";
import {console} from "forge-std/Script.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {WrappedVara} from "../src/WrappedVara.sol";

import {Middleware} from "../src/Middleware.sol";
import {IMiddleware} from "../src/IMiddleware.sol";
import {IDefaultOperatorRewardsFactory} from
    "symbiotic-rewards/src/interfaces/defaultOperatorRewards/IDefaultOperatorRewardsFactory.sol";

struct MiddlewareDeploymentArguments {
    // Addresses
    address vaultRegistry;
    address operatorRegistry;
    address networkRegistry;
    address middlewareService;
    address networkOptIn;
    address stakerRewardsFactory;
}

struct RouterDeploymentArguments {
    address deployer;
    address[] validatorsArray;
    uint256 aggregatedPublicKeyX;
    uint256 aggregatedPublicKeyY;
    bytes verifiableSecretSharingCommitment;
    address wrappedVaraAddress;
    address middlewareAddress;
    address mirrorAddress;
}

library ProtocolConstants {
    // General constants
    uint48 internal constant ERA_DURATION = 12 hours;
    uint48 internal constant ELECTION_DURATION = 2 hours;
    uint48 internal constant VALIDATION_DELAY = 5 minutes;

    // Middleware specific constants
    uint48 internal constant MIN_VAULT_EPOCH_DURATION = 2 hours;
    uint48 internal constant OPERATOR_GRACE_PERIOD = 5 minutes;
    uint48 internal constant VAULT_GRACE_PERIOD = 5 minutes;
    uint48 internal constant MIN_VETO_DURATION = 2 hours;
    uint48 internal constant MIN_SLASH_EXECUTION_DELAY = 5 minutes;
    uint64 internal constant ALLOWED_VAULT_IMPL_VERSION = 1;
    uint64 internal constant VETO_SLASHER_IMPL_TYPE = 1;
    uint256 internal constant MAX_RESOLVER_SET_EPOCHS_DELAY = 5 minutes;
    uint256 internal constant VAULT_MAX_ADMIN_FEE = 10000;
}

abstract contract BaseDeployment is CommonBase, StdAssertions, StdChains, StdCheats, StdInvariant, StdUtils {
    //////////////////////////////////////////////////////////////////
    // DEPLOYMENT FULL GEAR EXE
    //////////////////////////////////////////////////////////////////

    function deployGearExeFromEnvironment() public {

        vm.createSelectFork("hoodi");

        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address deployerAddress = vm.addr(privateKey);

        WrappedVara wvara = deployWrappedVaraFromEnvironment();

        address mirrorAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 2);
        address middlewareAddress = vm.computeCreateAddress(deployerAddress, vm.getNonce(deployerAddress) + 3);

        Router router = deployRouterFromEnvironment(middlewareAddress, mirrorAddress, address(wvara));
        Mirror mirror = deployMirror(address(router));

        if (!(vm.envExists("DEV_MODE") && vm.envBool("DEV_MODE"))) {
            Middleware middleware = deployMiddlewareFromEnvironment(address(router), address(wvara));

            // Check addresses computation validity
            vm.assertEq(router.middleware(), address(middleware));
        }

        wvara.approve(address(router), type(uint256).max);

        if (vm.envExists("SENDER_ADDRESS")) {
            address senderAddress = vm.envAddress("SENDER_ADDRESS");
            bool success = wvara.transfer(senderAddress, 500_000 * (10 ** wvara.decimals()));
            vm.assertTrue(success);
        }

        vm.roll(vm.getBlockNumber() + 1);
        router.lookupGenesisHash();

        vm.assertEq(router.mirrorImpl(), address(mirror));
        vm.assertNotEq(router.genesisBlockHash(), bytes32(0));
    }


    ///////////////////////////////////////////////////////////////////
    // DEPLOYMENT FROM ENVIRONMENT
    ///////////////////////////////////////////////////////////////////

    function deployWrappedVaraFromEnvironment() public returns (WrappedVara wrappedVara) {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address deployerAddress = vm.addr(privateKey);

        wrappedVara = deployWrappedVara(deployerAddress);
    }

    function deployRouterFromEnvironment(address middlewareAddress, address mirrorAddress, address wrappedVaraAddress)
        public
        returns (Router router)
    {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address[] memory validatorsArray = vm.envAddress("ROUTER_VALIDATORS_LIST", ",");
        uint256 aggregatedPublicKeyX = vm.envUint("ROUTER_AGGREGATED_PUBLIC_KEY_X");
        uint256 aggregatedPublicKeyY = vm.envUint("ROUTER_AGGREGATED_PUBLIC_KEY_Y");
        bytes memory verifiableSecretSharingCommitment = vm.envBytes("ROUTER_VERIFIABLE_SECRET_SHARING_COMMITMENT");
        address deployerAddress = vm.addr(privateKey);

        RouterDeploymentArguments memory args = RouterDeploymentArguments({
            deployer: deployerAddress,
            validatorsArray: validatorsArray,
            aggregatedPublicKeyX: vm.envUint("AGGREGATED_PUBLIC_KEY_X"),
            aggregatedPublicKeyY: vm.envUint("AGGREGATED_PUBLIC_KEY_Y"),
            verifiableSecretSharingCommitment: vm.envBytes("VERIFIABLE_SECRET_SHARING_COMMITMENT"),
            wrappedVaraAddress: wrappedVaraAddress,
            middlewareAddress: middlewareAddress,
            mirrorAddress: mirrorAddress
        });

        router = deployRouter(args);
    }

    function deployMiddlewareFromEnvironment(address routerAddress, address wrappedVaraAddress)
        public
        returns (Middleware middleware)
    {
        uint256 privateKey = vm.envUint("PRIVATE_KEY");
        address deployerAddress = vm.addr(privateKey);

        Gear.SymbioticContracts memory symbiotic = symbioticEcosystemFromEnvironment(routerAddress);

        IMiddleware.InitParams memory initParams = middlewareInitParamsFromProtocolConstants(
            deployerAddress, routerAddress, wrappedVaraAddress, symbiotic
        );

        middleware = deployMiddleware(deployerAddress, initParams);
    }

    ///////////////////////////////////////////////////////////////////
    // INTERNAL DEPLOYMENT HELPERS
    ///////////////////////////////////////////////////////////////////

    function deployRouter(RouterDeploymentArguments memory args) public returns (Router router) {
        router = Router(
            Upgrades.deployTransparentProxy(
                "Router.sol",
                args.deployer,
                abi.encodeCall(
                    Router.initialize,
                    (
                        args.deployer,
                        args.mirrorAddress,
                        args.wrappedVaraAddress,
                        args.middlewareAddress,
                        ProtocolConstants.ERA_DURATION,
                        ProtocolConstants.ELECTION_DURATION,
                        ProtocolConstants.VALIDATION_DELAY,
                        Gear.AggregatedPublicKey(args.aggregatedPublicKeyX, args.aggregatedPublicKeyY),
                        args.verifiableSecretSharingCommitment,
                        args.validatorsArray
                    )
                )
            )
        );

        emit RouterDeployed(address(router));
        printContractInfo("Router", address(router), Upgrades.getImplementationAddress(address(router)));
    }

    function deployWrappedVara(address deployerAddress) public returns (WrappedVara wrappedVara) {
        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", deployerAddress, abi.encodeCall(WrappedVara.initialize, (deployerAddress))
            )
        );

        emit WrappedVaraDeployed(address(wrappedVara));
        printContractInfo("WVara", address(wrappedVara), Upgrades.getImplementationAddress(address(wrappedVara)));
    }

    function deployMirror(address routerAddress) public returns (Mirror mirror) {
        mirror = new Mirror(address(routerAddress));

        emit MirrorDeployed(address(mirror));
        printContractInfo("Mirror", address(mirror), address(0));
    }

    function deployMiddleware(address deployerAddress, IMiddleware.InitParams memory initParams)
        public
        returns (Middleware middleware)
    {
        middleware = Middleware(
            Upgrades.deployTransparentProxy(
                "Middleware.sol", deployerAddress, abi.encodeCall(Middleware.initialize, (initParams))
            )
        );
        emit MiddlewareDeployed(address(middleware));
    }

    function middlewareInitParamsFromProtocolConstants(
        address deployerAddress,
        address routerAddress,
        address wrappedVaraAddress,
        Gear.SymbioticContracts memory symbiotic
    ) public returns (IMiddleware.InitParams memory initParams) {
        initParams = IMiddleware.InitParams({
            owner: deployerAddress,
            eraDuration: ProtocolConstants.ERA_DURATION,
            minVaultEpochDuration: ProtocolConstants.MIN_VAULT_EPOCH_DURATION,
            operatorGracePeriod: ProtocolConstants.OPERATOR_GRACE_PERIOD,
            vaultGracePeriod: ProtocolConstants.VAULT_GRACE_PERIOD,
            minVetoDuration: ProtocolConstants.MIN_VETO_DURATION,
            minSlashExecutionDelay: ProtocolConstants.MIN_SLASH_EXECUTION_DELAY,
            allowedVaultImplVersion: ProtocolConstants.ALLOWED_VAULT_IMPL_VERSION,
            vetoSlasherImplType: ProtocolConstants.VETO_SLASHER_IMPL_TYPE,
            maxResolverSetEpochsDelay: ProtocolConstants.MAX_RESOLVER_SET_EPOCHS_DELAY,
            collateral: wrappedVaraAddress,
            maxAdminFee: ProtocolConstants.VAULT_MAX_ADMIN_FEE,
            router: routerAddress,
            symbiotic: symbiotic
        });
    }

    function symbioticEcosystemFromEnvironment(address routerAddress)
        public
        returns (Gear.SymbioticContracts memory symbiotic)
    {
        address operatorRewardsFactoryAddress = vm.envAddress("SYMBIOTIC_OPERATOR_REWARDS_FACTORY");

        symbiotic = Gear.SymbioticContracts({
            vaultRegistry: vm.envAddress("SYMBIOTIC_VAULT_REGISTRY"),
            operatorRegistry: vm.envAddress("SYMBIOTIC_OPERATOR_REGISTRY"),
            networkRegistry: vm.envAddress("SYMBIOTIC_NETWORK_REGISTRY"),
            middlewareService: vm.envAddress("SYMBIOTIC_MIDDLEWARE_SERVICE"),
            networkOptIn: vm.envAddress("SYMBIOTIC_NETWORK_OPT_IN"),
            stakerRewardsFactory: vm.envAddress("SYMBIOTIC_STAKER_REWARDS_FACTORY"),
            operatorRewards: IDefaultOperatorRewardsFactory(vm.envAddress("SYMBIOTIC_OPERATOR_REWARDS_FACTORY")).create(),
            roleSlashRequester: routerAddress,
            roleSlashExecutor: routerAddress,
            vetoResolver: routerAddress
        });
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
            if (chainId == 1) {
                console.log("curl --request POST 'https://api.etherscan.io/api' \\");
            } else {
                console.log(
                    string.concat(
                        "curl --request POST 'https://api-", vm.getChain(chainId).chainAlias, ".etherscan.io/api' \\"
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

    event RouterDeployed(address routerAddress);
    event MiddlewareDeployed(address middlewareAddress);
    event WrappedVaraDeployed(address wrappedVaraAddress);
    event MirrorDeployed(address mirrorAddress);
}
