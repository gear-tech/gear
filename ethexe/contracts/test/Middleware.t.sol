// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {Upgrades} from "openzeppelin-foundry-upgrades/Upgrades.sol";
import {Test, console} from "forge-std/Test.sol";
import {Vm} from "forge-std/Vm.sol";

import {NetworkRegistry} from "symbiotic-core/src/contracts/NetworkRegistry.sol";
import {POCBaseTest} from "symbiotic-core/test/POCBase.t.sol";
import {IVaultConfigurator} from "symbiotic-core/src/interfaces/IVaultConfigurator.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {IOperatorSpecificDelegator} from "symbiotic-core/src/interfaces/delegator/IOperatorSpecificDelegator.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IBaseSlasher} from "symbiotic-core/src/interfaces/slasher/IBaseSlasher.sol";

import {Middleware} from "../src/Middleware.sol";
import {WrappedVara} from "../src/WrappedVara.sol";
import {MapWithTimeData} from "../src/libraries/MapWithTimeData.sol";

contract MiddlewareTest is Test, POCBaseTest {
    using MessageHashUtils for address;

    uint48 eraDuration = 1000;
    POCBaseTest public sym;
    Middleware public middleware;
    WrappedVara public wrappedVara;

    function setUp() public override {
        // For correct symbiotic work with time arithmetics
        vm.warp(eraDuration * 100);

        // set up the symbiotic ecosystem
        SYMBIOTIC_CORE_PROJECT_ROOT = "lib/symbiotic-core/";
        super.setUp();

        // For understanding where symbiotic ecosystem is using
        sym = POCBaseTest(address(this));

        wrappedVara = WrappedVara(
            Upgrades.deployTransparentProxy(
                "WrappedVara.sol", address(0xdead), abi.encodeCall(WrappedVara.initialize, (owner))
            )
        );

        wrappedVara.mint(owner, 1_000_000);

        Middleware.Config memory cfg = Middleware.Config({
            eraDuration: eraDuration,
            minVaultEpochDuration: eraDuration * 2,
            operatorGracePeriod: eraDuration * 2,
            vaultGracePeriod: eraDuration * 2,
            minVetoDuration: eraDuration / 3,
            minSlashExecutionDelay: eraDuration / 3,
            maxResolverSetEpochsDelay: type(uint256).max,
            vaultRegistry: address(sym.vaultFactory()),
            allowedVaultImplVersion: 1,
            vetoSlasherImplType: 1,
            operatorRegistry: address(sym.operatorRegistry()),
            networkRegistry: address(sym.networkRegistry()),
            networkOptIn: address(sym.operatorNetworkOptInService()),
            middlewareService: address(sym.networkMiddlewareService()),
            collateral: address(wrappedVara),
            roleSlashRequester: owner,
            roleSlashExecutor: owner,
            vetoResolver: owner
        });

        middleware = new Middleware(cfg);
    }

    // TODO: sync with the latest version of the middleware
    function test_constructor() public view {
        assertTrue(sym.networkRegistry().isEntity(address(middleware)));
        assertEq(sym.networkMiddlewareService().middleware(address(middleware)), address(middleware));
    }

    function test_election() public {
        address[] memory operators = new address[](5);
        address[] memory vaults = new address[](operators.length);

        for (uint256 i = 0; i < operators.length; i++) {
            operators[i] = address(uint160(i) + 0x1000);
            _createOperator(operators[i]);
            vaults[i] = _createVaultForOperator(operators[i]);
        }

        _depositFromInVault(owner, vaults[0], 1_000);
        _depositFromInVault(owner, vaults[1], 2_000);
        _depositFromInVault(owner, vaults[2], 1_000);
        _depositFromInVault(owner, vaults[3], 5_000);
        _depositFromInVault(owner, vaults[4], 1_000);

        vm.warp(vm.getBlockTimestamp() + 1000);

        vm.expectRevert(abi.encodeWithSelector(Middleware.IncorrectTimestamp.selector));
        middleware.makeElectionAt(uint48(vm.getBlockTimestamp()), 10);

        vm.expectRevert();
        middleware.makeElectionAt(uint48(vm.getBlockTimestamp()) - 1, 0);

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 1);
            assertEq(res.length, 1);
            assertEq(res[0], operators[3]);
        }

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 2);
            assertEq(res.length, 2);
            assertEq(res[0], operators[3]);
            assertEq(res[1], operators[1]);
        }

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 3);
            assertEq(res.length, 3);
            assertEq(res[0], operators[3]);
            assertEq(res[1], operators[1]);
            assertTrue(res[2] == operators[0] || res[2] == operators[2] || res[2] == operators[4]);
        }

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 4);
            assertEq(res.length, 4);
            assertEq(res[0], operators[3]);
            assertEq(res[1], operators[1]);
            assertTrue(res[2] == operators[0] || res[2] == operators[2] || res[2] == operators[4]);
            assertTrue((res[3] == operators[3] || res[3] == operators[2] || res[3] == operators[4]) && res[3] != res[2]);
        }

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 5);
            assertEq(res.length, operators.length);
            // In that case not sorted by stake
            for (uint256 i; i < operators.length; i++) {
                assertEq(res[i], operators[i]);
            }
        }

        {
            address[] memory res = middleware.makeElectionAt(uint48(vm.getBlockTimestamp() - 1), 6);
            assertEq(res.length, operators.length);
            // In that case not sorted by stake
            for (uint256 i; i < operators.length; i++) {
                assertEq(res[i], operators[i]);
            }
        }
    }

    // TODO: split to multiple tests
    function test_registerOperator() public {
        // Register operator
        vm.startPrank(address(0x1));
        sym.operatorRegistry().registerOperator();
        sym.operatorNetworkOptInService().optIn(address(middleware));
        middleware.registerOperator();

        // Try to register operator again
        vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyAdded.selector));
        middleware.registerOperator();

        // Try to register another operator without registering it in symbiotic
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

        // Disable operator and then enable it
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
        middleware.unregisterOperator(address(0x2));

        // Wait for grace period and unregister operator from other address
        vm.startPrank(address(0x3));
        vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
        middleware.unregisterOperator(address(0x2));
    }

    // TODO: split to multiple tests
    // TODO: check vault has valid network params
    // TODO: test when vault has incorrect network params
    function test_registerVault() public {
        sym.operatorRegistry().registerOperator();
        address vault = _newVault(eraDuration * 2, owner);

        // Register vault
        middleware.registerVault(vault);

        // Try to register unknown vault
        vm.expectRevert(abi.encodeWithSelector(Middleware.NotKnownVault.selector));
        middleware.registerVault(address(0xdead));

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
        vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
        middleware.unregisterVault(vault);

        // Register vault again, disable and unregister it not by owner
        middleware.registerVault(vault);
        middleware.disableVault(vault);
        vm.startPrank(address(0x1));
        vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
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

    function test_stake() public {
        (address operator1, address operator2,,, uint256 stake1, uint256 stake2) = _prepareTwoOperators();

        uint48 ts = uint48(vm.getBlockTimestamp() - 1);

        // Check operator stake after depositing
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake1);
        assertEq(middleware.getOperatorStakeAt(operator2, ts), stake2);

        // Check active operators
        (address[] memory activeOperators, uint256[] memory stakes) = middleware.getActiveOperatorsStakeAt(ts);
        assertEq(activeOperators.length, 2);
        assertEq(stakes.length, 2);
        assertEq(activeOperators[0], operator1);
        assertEq(activeOperators[1], operator2);
        assertEq(stakes[0], stake1);
        assertEq(stakes[1], stake2);
    }

    function test_stakeOperatorWithTwoVaults() public {
        (address operator1,, address vault1,, uint256 stake1,) = _prepareTwoOperators();

        // Create one more vault for operator1
        address vault3 = _createVaultForOperator(operator1);

        // Check that vault creation doesn't affect operator stake without deposit
        uint48 ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake1);

        // Check after depositing to new vault
        uint256 stake3 = 3_000;
        _depositFromInVault(owner, vault3, stake3);
        ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake1 + stake3);

        // Disable vault1 and check operator1 stake
        _disableVault(operator1, vault1);
        // Disable is not immediate, so we need to check for the next block ts
        ts = uint48(vm.getBlockTimestamp()) + 1;
        vm.warp(vm.getBlockTimestamp() + 2);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake3);
    }

    function test_stakeDisabledOperator() public {
        (address operator1, address operator2,,,, uint256 stake2) = _prepareTwoOperators();

        // Disable operator1 and check operator1 stake is 0
        _disableOperator(operator1);
        // Disable is not immediate, so we need to check for the next block ts
        uint48 ts = uint48(vm.getBlockTimestamp()) + 1;
        vm.warp(vm.getBlockTimestamp() + 2);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), 0);

        // Check that operator1 is not in active operators list
        (address[] memory active_operators, uint256[] memory stakes) = middleware.getActiveOperatorsStakeAt(ts);
        assertEq(active_operators.length, 1);
        assertEq(stakes.length, 1);
        assertEq(active_operators[0], operator2);
        assertEq(stakes[0], stake2);
    }

    function test_stakeTooOldTimestamp() public {
        (address operator1,,,,,) = _prepareTwoOperators();

        // Try to get stake for too old timestamp
        uint48 ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
        vm.expectRevert(abi.encodeWithSelector(Middleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, ts);
    }

    function test_stakeCurrentTimestamp() public {
        (address operator1,,,,,) = _prepareTwoOperators();

        // Try to get stake for current timestamp
        vm.expectRevert(abi.encodeWithSelector(Middleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, uint48(vm.getBlockTimestamp()));
    }

    function test_stakeFutureTimestamp() public {
        (address operator1,,,,,) = _prepareTwoOperators();

        // Try to get stake for future timestamp
        vm.expectRevert(abi.encodeWithSelector(Middleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, uint48(vm.getBlockTimestamp() + 1));
    }

    function test_slash() external {
        (address operator1,, address vault1,, uint256 stake1,) = _prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = _requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);
        uint48 vetoDeadline = _vetoDeadline(IVault(vault1).slasher(), slashIndex);
        assertEq(vetoDeadline, uint48(vm.getBlockTimestamp() + eraDuration / 2));

        // Try to execute slash before veto deadline
        vm.warp(vetoDeadline - 1);
        vm.expectRevert(IVetoSlasher.VetoPeriodNotEnded.selector);
        _executeSlash(vault1, slashIndex);

        // Execute slash when ready
        vm.warp(vetoDeadline);
        _executeSlash(vault1, slashIndex);

        // Check that operator1 stake is decreased
        vm.warp(vetoDeadline + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, vetoDeadline), stake1 - 100);

        // Try to execute slash twice
        vm.expectRevert(IVetoSlasher.SlashRequestCompleted.selector);
        _executeSlash(vault1, slashIndex);
    }

    function test_slashRequestUnknownOperator() external {
        (,, address vault1,,,) = _prepareTwoOperators();

        // Try to request slash from unknown operator
        vm.warp(vm.getBlockTimestamp() + 1);
        _requestSlash(
            address(0xdead), uint48(vm.getBlockTimestamp() - 1), vault1, 100, Middleware.NotRegisteredOperator.selector
        );
    }

    function test_slashRequestUnknownVault() external {
        (address operator1,,,,,) = _prepareTwoOperators();

        // Try to request slash from unknown vault
        _requestSlash(
            operator1, uint48(vm.getBlockTimestamp() - 1), address(0xdead), 100, Middleware.NotRegisteredVault.selector
        );
    }

    function test_slashRequestOnVaultWithNoStake() external {
        (address operator1,,, address vault2,,) = _prepareTwoOperators();

        // Try to request slash on vault where it has no stake
        _requestSlash(
            operator1, uint48(vm.getBlockTimestamp() - 1), vault2, 10, IVetoSlasher.InsufficientSlash.selector
        );
    }

    function test_slashAfterSlashPeriod() external {
        (address operator1,, address vault1,,,) = _prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = _requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);

        // Try to slash after slash period
        vm.warp(uint48(vm.getBlockTimestamp()) + IVault(vault1).epochDuration());
        vm.expectRevert(IVetoSlasher.SlashPeriodEnded.selector);
        _executeSlash(vault1, slashIndex);
    }

    function test_slashOneOperatorTwoVaults() external {
        (address operator1,, address vault1, address vault2,,) = _prepareTwoOperators();

        // Try request slashes for one operator, but 2 vaults
        Middleware.VaultSlashData[] memory vaults = new Middleware.VaultSlashData[](2);
        vaults[0] = Middleware.VaultSlashData({vault: vault1, amount: 10});
        vaults[1] = Middleware.VaultSlashData({vault: vault2, amount: 20});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](1);
        slashes[0] = Middleware.SlashData({operator: operator1, ts: uint48(vm.getBlockTimestamp() - 1), vaults: vaults});

        _requestSlash(slashes, IVetoSlasher.InsufficientSlash.selector);

        // Make one more vault for operator1
        address vault3 = _createVaultForOperator(operator1);
        _depositFromInVault(owner, vault3, 3_000);

        vm.warp(vm.getBlockTimestamp() + 1);

        // Request slashes with correct vaults
        vaults[1] = Middleware.VaultSlashData({vault: vault3, amount: 30});
        slashes[0] = Middleware.SlashData({operator: operator1, ts: uint48(vm.getBlockTimestamp() - 1), vaults: vaults});
        _requestSlash(slashes, 0);
    }

    function test_slashTwoOperatorsTwoVaults() external {
        (address operator1, address operator2, address vault1, address vault2,,) = _prepareTwoOperators();

        // Request slashes for 2 operators with corresponding vaults
        Middleware.VaultSlashData[] memory operator1_vaults = new Middleware.VaultSlashData[](1);
        operator1_vaults[0] = Middleware.VaultSlashData({vault: vault1, amount: 10});

        Middleware.VaultSlashData[] memory operator2_vaults = new Middleware.VaultSlashData[](1);
        operator2_vaults[0] = Middleware.VaultSlashData({vault: vault2, amount: 20});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](2);
        slashes[0] = Middleware.SlashData({
            operator: operator1,
            ts: uint48(vm.getBlockTimestamp() - 1),
            vaults: operator1_vaults
        });
        slashes[1] = Middleware.SlashData({
            operator: operator2,
            ts: uint48(vm.getBlockTimestamp() - 1),
            vaults: operator2_vaults
        });

        _requestSlash(slashes, 0);
    }

    function test_slashVeto() external {
        (address operator1,, address vault1,,,) = _prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = _requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);
        uint48 vetoDeadline = _vetoDeadline(IVault(vault1).slasher(), slashIndex);

        address slasher = IVault(vault1).slasher();

        // Try to execute slash after veto deadline
        vm.warp(vetoDeadline);
        vm.expectRevert(IVetoSlasher.VetoPeriodEnded.selector);
        IVetoSlasher(slasher).vetoSlash(slashIndex, new bytes(0));

        // Veto slash
        vm.warp(vetoDeadline - 1);
        IVetoSlasher(slasher).vetoSlash(slashIndex, new bytes(0));

        // Try to execute slash after veto is done
        vm.expectRevert(IVetoSlasher.SlashRequestCompleted.selector);
        IVetoSlasher(slasher).vetoSlash(slashIndex, new bytes(0));
    }

    function test_slashExecutionUnregisteredVault() external {
        (address operator1,, address vault1,,,) = _prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = _requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);

        // Try to execute slash for unknown vault
        vm.expectRevert(Middleware.NotRegisteredVault.selector);
        _executeSlash(address(0xdead), slashIndex);
    }

    function _executeSlash(address vault, uint256 index) private {
        Middleware.SlashIdentifier[] memory slashes = new Middleware.SlashIdentifier[](1);
        slashes[0] = Middleware.SlashIdentifier({vault: vault, index: index});
        middleware.executeSlash(slashes);
    }

    function _prepareTwoOperators()
        private
        returns (address operator1, address operator2, address vault1, address vault2, uint256 stake1, uint256 stake2)
    {
        operator1 = address(0x1);
        operator2 = address(0x2);

        _createOperator(operator1);
        _createOperator(operator2);

        vault1 = _createVaultForOperator(operator1);
        vault2 = _createVaultForOperator(operator2);

        stake1 = 1_000;
        stake2 = 2_000;

        _depositFromInVault(owner, vault1, stake1);
        _depositFromInVault(owner, vault2, stake2);

        vm.warp(vm.getBlockTimestamp() + 1);
    }

    function _vetoDeadline(address slasher, uint256 slash_index) private view returns (uint48) {
        (,,,, uint48 vetoDeadline,) = IVetoSlasher(slasher).slashRequests(slash_index);
        return vetoDeadline;
    }

    function _requestSlash(address operator, uint48 ts, address vault, uint256 amount, bytes4 err)
        private
        returns (uint256 slashIndex)
    {
        Middleware.VaultSlashData[] memory vaults = new Middleware.VaultSlashData[](1);
        vaults[0] = Middleware.VaultSlashData({vault: vault, amount: amount});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](1);
        slashes[0] = Middleware.SlashData({operator: operator, ts: ts, vaults: vaults});

        slashIndex = _requestSlash(slashes, err)[0];
        assertNotEq(slashIndex, type(uint256).max);
    }

    function _requestSlash(Middleware.SlashData[] memory slashes, bytes4 err)
        private
        returns (uint256[] memory slashIndexes)
    {
        uint256 len = 0;
        for (uint256 i = 0; i < slashes.length; i++) {
            len += slashes[i].vaults.length;
        }

        slashIndexes = new uint256[](len);

        vm.recordLogs();
        if (err != 0) {
            vm.expectRevert(err);
            middleware.requestSlash(slashes);
            return slashIndexes;
        } else {
            middleware.requestSlash(slashes);
        }
        Vm.Log[] memory logs = vm.getRecordedLogs();

        uint16 k = 0;
        for (uint256 i = 0; i < logs.length; i++) {
            Vm.Log memory log = logs[i];
            bytes32 eventSignature = log.topics[0];
            if (eventSignature == IVetoSlasher.RequestSlash.selector) {
                slashIndexes[k++] = uint256(log.topics[1]);
            }
        }
    }

    function _disableOperator(address operator) private {
        vm.startPrank(operator);
        middleware.disableOperator();
        vm.stopPrank();
    }

    function _disableVault(address vault_owner, address vault) private {
        vm.startPrank(vault_owner);
        middleware.disableVault(vault);
        vm.stopPrank();
    }

    function _depositFromInVault(address from, address vault, uint256 amount) private {
        vm.startPrank(from);
        wrappedVara.approve(vault, amount);
        IVault(vault).deposit(from, amount);
        vm.stopPrank();
    }

    function _createOperator(address operator) internal {
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

    function _newVault(uint48 epochDuration, address operator) private returns (address vault) {
        address[] memory networkLimitSetRoleHolders = new address[](1);
        networkLimitSetRoleHolders[0] = operator;

        (vault,,) = sym.vaultConfigurator().create(
            IVaultConfigurator.InitParams({
                version: 1,
                owner: operator,
                vaultParams: abi.encode(
                    IVault.InitParams({
                        collateral: address(wrappedVara),
                        burner: address(middleware),
                        epochDuration: epochDuration,
                        depositWhitelist: false,
                        isDepositLimit: false,
                        depositLimit: 0,
                        defaultAdminRoleHolder: operator,
                        depositWhitelistSetRoleHolder: operator,
                        depositorWhitelistRoleHolder: operator,
                        isDepositLimitSetRoleHolder: operator,
                        depositLimitSetRoleHolder: operator
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
                withSlasher: true,
                slasherIndex: 1,
                slasherParams: abi.encode(
                    IVetoSlasher.InitParams({
                        baseParams: IBaseSlasher.BaseParams({isBurnerHook: false}),
                        vetoDuration: eraDuration / 2,
                        resolverSetEpochsDelay: 3
                    })
                )
            })
        );
    }
}
