// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";

import {Test, console} from "forge-std/Test.sol";
import {NetworkRegistry} from "symbiotic-core/src/contracts/NetworkRegistry.sol";
import {POCBaseTest} from "symbiotic-core/test/POCBase.t.sol";
import {IVaultConfigurator} from "symbiotic-core/src/interfaces/IVaultConfigurator.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {IOperatorSpecificDelegator} from "symbiotic-core/src/interfaces/delegator/IOperatorSpecificDelegator.sol";
// import {IOptInService} from "symbiotic-core/src/interfaces/service/IOptInService.sol";

import {Middleware} from "../src/Middleware.sol";
import {WrappedVara} from "../src/WrappedVara.sol";
import {MapWithTimeData} from "../src/libraries/MapWithTimeData.sol";

contract MiddlewareTest is Test {
    using MessageHashUtils for address;

    uint48 eraDuration = 1000;
    address public owner;
    POCBaseTest public sym;
    Middleware public middleware;
    WrappedVara public wrappedVara;

    function setUp() public {
        sym = new POCBaseTest();
        sym.setUp();

        owner = address(this);

        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy("WrappedVara.sol", owner, abi.encodeCall(WrappedVara.initialize, (owner)))
        );

        wrappedVara.mint(owner, 1_000_000);

        middleware = new Middleware(
            eraDuration,
            address(sym.vaultFactory()),
            address(sym.delegatorFactory()),
            address(sym.slasherFactory()),
            address(sym.operatorRegistry()),
            address(sym.networkRegistry()),
            address(sym.operatorNetworkOptInService()),
            address(wrappedVara)
        );
    }

    function test_constructor() public view {
        assertEq(uint256(middleware.ERA_DURATION()), eraDuration);
        assertEq(uint256(middleware.GENESIS_TIMESTAMP()), Time.timestamp());
        assertEq(middleware.VAULT_FACTORY(), address(sym.vaultFactory()));
        assertEq(middleware.DELEGATOR_FACTORY(), address(sym.delegatorFactory()));
        assertEq(middleware.SLASHER_FACTORY(), address(sym.slasherFactory()));
        assertEq(middleware.OPERATOR_REGISTRY(), address(sym.operatorRegistry()));
        assertEq(middleware.COLLATERAL(), address(wrappedVara));

        sym.networkRegistry().isEntity(address(middleware));
    }

    function test_registerOperator() public {
        // Register operator
        vm.startPrank(address(0x1));
        sym.operatorRegistry().registerOperator();
        sym.operatorNetworkOptInService().optIn(address(middleware));
        middleware.registerOperator();

        // Try to register operator again
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyAdded.selector));
        middleware.registerOperator();

        // Try to register abother operator without registering it in symbiotic
        vm.startPrank(address(0x2));
        vm.expectRevert(abi.encodeWithSelector(Middleware.OperatorDoesNotExist.selector));
        middleware.registerOperator();

        // Try to register operator without opting in network
        sym.operatorRegistry().registerOperator();
        vm.expectRevert(abi.encodeWithSelector(Middleware.OperatorDoesNotOptIn.selector));
        middleware.registerOperator();

        // Now must be possible to register operator
        sym.operatorNetworkOptInService().optIn(address(middleware));
        middleware.registerOperator();

        // Disable operator and the enable it
        middleware.disableOperator();
        middleware.enableOperator();

        // Try to enable operator again
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyEnabled.selector));
        middleware.enableOperator();

        // Try to disable operator twice
        middleware.disableOperator();
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.NotEnabled.selector));
        middleware.disableOperator();

        // Try to unregister operator - failed because operator is not disabled for enough time
        vm.expectRevert(abi.encodeWithSelector(Middleware.OperatorGracePeriodNotPassed.selector));
        middleware.unregisterOperator();

        // Wait for grace period and unregister operator
        vm.warp(block.timestamp + eraDuration * 2);
        middleware.unregisterOperator();
    }

    function test_registerVault() public {
        sym.operatorRegistry().registerOperator();
        address vault = _newVault(eraDuration * 2, owner);

        // Register vault
        middleware.registerVault(vault);

        // Try to register vault with zero address
        vm.expectRevert(abi.encodeWithSelector(Middleware.ZeroVaultAddress.selector));
        middleware.registerVault(address(0x0));

        // Try to register unknown vault
        vm.expectRevert(abi.encodeWithSelector(Middleware.NotKnownVault.selector));
        middleware.registerVault(address(0x1));

        // Try to register vault with wrong epoch duration
        address vault2 = _newVault(eraDuration, owner);
        vm.expectRevert(abi.encodeWithSelector(Middleware.VaultWrongEpochDuration.selector));
        middleware.registerVault(vault2);

        // Try to register vault with unknown collateral
        address vault3 = address(sym.vault1());
        vm.expectRevert(abi.encodeWithSelector(Middleware.UnknownCollateral.selector));
        middleware.registerVault(vault3);

        // Try to enable vault once more
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyEnabled.selector));
        middleware.enableVault(vault);

        // Try to disable vault twice
        middleware.disableVault(vault);
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.NotEnabled.selector));
        middleware.disableVault(vault);

        {
            vm.startPrank(address(0x1));

            // Try to enable vault not from owner
            vm.expectRevert(abi.encodeWithSelector(Middleware.NotVaultOwner.selector));
            middleware.enableVault(vault);

            // Try to disable vault not from owner
            vm.expectRevert(abi.encodeWithSelector(Middleware.NotVaultOwner.selector));
            middleware.disableVault(vault);

            vm.stopPrank();
        }

        // Try to unregister vault - failed because vault is not disabled for enough time
        vm.expectRevert(abi.encodeWithSelector(Middleware.VaultGracePeriodNotPassed.selector));
        middleware.unregisterVault(vault);

        // Wait for grace period and unregister vault
        vm.warp(block.timestamp + eraDuration * 2);
        middleware.unregisterVault(vault);

        // Register vault again, disable and unregister it not by owner
        middleware.registerVault(vault);
        middleware.disableVault(vault);
        vm.startPrank(address(0x1));
        vm.warp(block.timestamp + eraDuration * 2);
        middleware.unregisterVault(vault);
        vm.stopPrank();

        // Try to enable unknown vault
        vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, address(0x1)));
        middleware.enableVault(address(0x1));

        // Try to disable unknown vault
        vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, address(0x1)));
        middleware.disableVault(address(0x1));

        // Try to unregister unknown vault
        vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, address(0x1)));
        middleware.unregisterVault(address(0x1));
    }

    function test_operatorStake() public {
        address operator1 = address(0x1);
        address operator2 = address(0x2);

        _registerOperator(operator1);
        _registerOperator(operator2);

        address vault1 = _createVaultForOperator(operator1);
        address vault2 = _createVaultForOperator(operator2);

        _depositFromInVault(owner, vault1, 1_000);
        _depositFromInVault(owner, vault2, 2_000);

        uint48 ts1 = uint48(block.timestamp);
        vm.warp(block.timestamp + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts1), 1_000);
        assertEq(middleware.getOperatorStakeAt(operator2, ts1), 2_000);

        // Create one more vault for operator1 and check operator1 stake
        address vault3 = _createVaultForOperator(operator1);

        uint48 ts2 = uint48(block.timestamp);
        vm.warp(block.timestamp + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts2), 1_000);

        _depositFromInVault(owner, vault3, 3_000);
        uint48 ts3 = uint48(block.timestamp);
        vm.warp(block.timestamp + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts3), 4_000);
    }

    function _depositFromInVault(address from, address vault, uint256 amount) private {
        vm.startPrank(from);
        wrappedVara.approve(vault, amount);
        IVault(vault).deposit(from, amount);
        vm.stopPrank();
    }

    function _registerOperator(address operator) private {
        vm.startPrank(operator);
        sym.operatorRegistry().registerOperator();
        sym.operatorNetworkOptInService().optIn(address(middleware));
        middleware.registerOperator();
        vm.stopPrank();
    }

    function _createVaultForOperator(address operator) private returns (address vault) {
        // Create vault
        vault = _newVault(eraDuration * 2, operator);
        {
            vm.startPrank(operator);

            // Register vault in middleware
            middleware.registerVault(vault);

            // Operator opt-in vault
            sym.operatorVaultOptInService().optIn(vault);

            // Set initial network limit
            IOperatorSpecificDelegator(IVault(vault).delegator()).setNetworkLimit(
                middleware.subnetwork(), type(uint256).max
            );

            vm.stopPrank();
        }
    }

    function _setNetworkLimit(address vault, address operator, uint256 limit) private {
        vm.startPrank(address(operator));
        IOperatorSpecificDelegator(IVault(vault).delegator()).setNetworkLimit(middleware.subnetwork(), limit);
        vm.stopPrank();
    }

    function _newVault(uint48 epochDuration, address operator) private returns (address vault) {
        address[] memory networkLimitSetRoleHolders = new address[](1);
        networkLimitSetRoleHolders[0] = operator;

        (vault,,) = sym.vaultConfigurator().create(
            IVaultConfigurator.InitParams({
                version: sym.vaultFactory().lastVersion(),
                owner: owner,
                vaultParams: abi.encode(
                    IVault.InitParams({
                        collateral: address(wrappedVara),
                        burner: address(middleware),
                        epochDuration: epochDuration,
                        depositWhitelist: false,
                        isDepositLimit: false,
                        depositLimit: 0,
                        defaultAdminRoleHolder: owner,
                        depositWhitelistSetRoleHolder: owner,
                        depositorWhitelistRoleHolder: owner,
                        isDepositLimitSetRoleHolder: owner,
                        depositLimitSetRoleHolder: owner
                    })
                ),
                delegatorIndex: 2,
                delegatorParams: abi.encode(
                    IOperatorSpecificDelegator.InitParams({
                        baseParams: IBaseDelegator.BaseParams({
                            defaultAdminRoleHolder: operator,
                            hook: address(0),
                            hookSetRoleHolder: operator
                        }),
                        networkLimitSetRoleHolders: networkLimitSetRoleHolders,
                        operator: operator
                    })
                ),
                withSlasher: false,
                slasherIndex: 0,
                slasherParams: bytes("")
            })
        );
    }
}