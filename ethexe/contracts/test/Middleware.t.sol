// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {Base} from "./Base.t.sol";
import {MessageHashUtils} from "@openzeppelin/contracts/utils/cryptography/MessageHashUtils.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Vm} from "forge-std/Vm.sol";
import {IMiddleware} from "src/IMiddleware.sol";
import {Middleware} from "src/Middleware.sol";
import {MapWithTimeData} from "src/libraries/MapWithTimeData.sol";
import {IVaultFactory} from "symbiotic-core/src/interfaces/IVaultFactory.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {POCBaseTest} from "symbiotic-core/test/POCBase.t.sol";

contract MiddlewareTest is Base {
    using MessageHashUtils for address;

    POCBaseTest private sym;

    function setUp() public override {
        admin = 0x116B4369a90d2E9DA6BD7a924A23B164E10f6FE9;
        eraDuration = 1000;

        setUpWrappedVara();
        setUpMiddleware();

        // For understanding where symbiotic ecosystem is using
        sym = POCBaseTest(address(this));
    }

    // TODO: sync with the latest version of the middleware
    function test_constructor() public view {
        assertTrue(sym.networkRegistry().isEntity(address(middleware)));
        assertEq(sym.networkMiddlewareService().middleware(address(middleware)), address(middleware));
    }

    function test_election() public {
        address[] memory operators = new address[](5);
        address[] memory vaults = new address[](operators.length);

        for (uint160 i = 0; i < operators.length; i++) {
            operators[i] = address(i + 0x1000);
            vaults[i] = createOperatorWithStake(operators[i], 1_000);
        }

        // Additional deposit for some operators
        depositInto(vaults[1], 1_000);
        depositInto(vaults[3], 4_000);

        vm.warp(vm.getBlockTimestamp() + 1000);

        vm.expectRevert(abi.encodeWithSelector(IMiddleware.IncorrectTimestamp.selector));
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
            assertTrue((res[3] == operators[0] || res[3] == operators[2] || res[3] == operators[4]) && res[3] != res[2]);
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
        {
            sym.operatorRegistry().registerOperator();
            sym.operatorNetworkOptInService().optIn(address(middleware));
            middleware.registerOperator();

            // Try to register operator again
            vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyAdded.selector));
            middleware.registerOperator();
        }
        vm.stopPrank();

        // Try to register another operator without registering it in symbiotic
        vm.startPrank(address(0x2));
        {
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.OperatorDoesNotExist.selector));
            middleware.registerOperator();

            // Try to register operator without opting in network
            sym.operatorRegistry().registerOperator();
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.OperatorDoesNotOptIn.selector));
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
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.OperatorGracePeriodNotPassed.selector));
            middleware.unregisterOperator(address(0x2));
        }
        vm.stopPrank();

        // Wait for grace period and unregister operator from other address
        vm.startPrank(address(0x3));
        {
            vm.warp(vm.getBlockTimestamp() + middleware.operatorGracePeriod());
            middleware.unregisterOperator(address(0x2));
        }
        vm.stopPrank();
    }

    // TODO: split to multiple tests
    // TODO: check vault has valid network params
    function test_registerVault() public {
        address _operator = address(0x1);

        vm.startPrank(_operator);
        {
            sym.operatorRegistry().registerOperator();
        }
        vm.stopPrank();

        address vault = newVault(_operator, _defaultVaultInitParams(_operator));
        address _rewards = newStakerRewards(vault, _operator);

        // Register vault
        // Note: because we set operator as vault admin we use `vm.startPrank()` to change function caller
        vm.startPrank(_operator);
        {
            middleware.registerVault(vault, newStakerRewards(vault, _operator));
        }
        vm.stopPrank();

        vm.startPrank(_operator);
        {
            // Try to enable vault once more
            vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.AlreadyEnabled.selector));
            middleware.enableVault(vault);

            // Try to disable vault twice
            middleware.disableVault(vault);
            vm.expectRevert(abi.encodeWithSelector(MapWithTimeData.NotEnabled.selector));
            middleware.disableVault(vault);
        }
        vm.stopPrank();

        // Try to register vault with wrong parameters
        vm.startPrank(_operator);
        {
            IVault.InitParams memory initParams = _defaultVaultInitParams(_operator);
            initParams.epochDuration = eraDuration;
            address vault2 = newVault(_operator, initParams);

            // Register vault with wrong epoch duration
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.VaultWrongEpochDuration.selector));
            middleware.registerVault(vault2, _rewards);

            // Make eraDuration correct, but collateral doesn't
            initParams.epochDuration = eraDuration * 2;
            initParams.collateral = address(0xabc);
            address vault3 = newVault(_operator, initParams);

            // Try to register vault with unknown collateral
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.UnknownCollateral.selector));
            middleware.registerVault(vault3, _rewards);
        }
        vm.stopPrank();

        // Try to register vault by not its owner
        vm.expectRevert(abi.encodeWithSelector(IMiddleware.NotVaultOwner.selector));
        middleware.registerVault(vault, _rewards);

        vm.startPrank(address(0xdead));
        {
            // Try to enable vault not from vault owner
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.NotVaultOwner.selector));
            middleware.enableVault(vault);

            // Try to disable vault not from vault owner
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.NotVaultOwner.selector));
            middleware.disableVault(vault);
        }
        vm.stopPrank();

        // Try to unregister vault - failed because vault is not disabled for enough time
        vm.startPrank(_operator);
        {
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.VaultGracePeriodNotPassed.selector));
            middleware.unregisterVault(vault);

            // Wait for grace period and unregister vault
            vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
            middleware.unregisterVault(vault);
        }
        vm.stopPrank();

        // Register vault again, disable and unregister it not by vault owner
        vm.startPrank(_operator);
        {
            middleware.registerVault(vault, _rewards);
            middleware.disableVault(vault);
        }
        vm.stopPrank();

        vm.startPrank(address(0xdead));
        {
            vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.NotVaultOwner.selector));
            middleware.unregisterVault(vault);
        }
        vm.stopPrank();

        address unknownVault = newVault(_operator, _defaultVaultInitParams(_operator));
        vm.startPrank(_operator);
        {
            // Try to enable unknown vault
            vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, unknownVault));
            middleware.enableVault(unknownVault);

            // Try to disable unknown vault
            vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, unknownVault));
            middleware.disableVault(unknownVault);

            // Try to unregister unknown vault
            vm.expectRevert(abi.encodeWithSelector(EnumerableMap.EnumerableMapNonexistentKey.selector, unknownVault));
            middleware.unregisterVault(unknownVault);
        }
        vm.stopPrank();

        /* Try to register vault from another factory*/

        IVaultFactory vaultFactory2 = IVaultFactory(
            deployCode(
                string.concat(SYMBIOTIC_CORE_PROJECT_ROOT, "out/VaultFactory.sol/VaultFactory.json"), abi.encode(owner)
            )
        );

        address vaultImpl = deployCode(
            string.concat(SYMBIOTIC_CORE_PROJECT_ROOT, "out/Vault.sol/Vault.json"),
            abi.encode(address(delegatorFactory), address(slasherFactory), address(vaultFactory2))
        );

        vaultFactory2.whitelist(vaultImpl);

        address vaultFromAnotherFactory =
            vaultFactory2.create(1, _operator, abi.encode(_defaultVaultInitParams(_operator)));

        vm.startPrank(_operator);
        {
            vm.expectRevert(abi.encodeWithSelector(IMiddleware.NonFactoryVault.selector));
            middleware.registerVault(vaultFromAnotherFactory, _rewards);
        }
        vm.stopPrank();
    }

    function test_stake() public {
        (address operator1, address operator2,,, uint256 stake1, uint256 stake2) = prepareTwoOperators();

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
        (address operator1,, address vault1,, uint256 stake1,) = prepareTwoOperators();

        // Create one more vault for operator1
        address vault3 = createVaultForOperator(operator1);

        // Check that vault creation doesn't affect operator stake without deposit
        uint48 ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake1);

        // Check after depositing to new vault
        uint256 stake3 = 3_000;
        depositInto(vault3, stake3);
        ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake1 + stake3);

        // Disable vault1 and check operator1 stake
        vm.startPrank(operator1);
        {
            middleware.disableVault(vault1);
        }
        vm.stopPrank();

        // Disable is not immediate, so we need to check for the next block ts
        ts = uint48(vm.getBlockTimestamp()) + 1;
        vm.warp(vm.getBlockTimestamp() + 2);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), stake3);
    }

    function test_stakeDisabledOperator() public {
        (address operator1, address operator2,,,, uint256 stake2) = prepareTwoOperators();

        // Disable operator1 and check operator1 stake is 0
        vm.startPrank(operator1);
        {
            middleware.disableOperator();
        }
        vm.stopPrank();

        // Disable is not immediate, so we need to check for the next block ts
        uint48 ts = uint48(vm.getBlockTimestamp()) + 1;
        vm.warp(vm.getBlockTimestamp() + 2);
        assertEq(middleware.getOperatorStakeAt(operator1, ts), 0);

        // Check that operator1 is not in active operators list
        (address[] memory activeOperators, uint256[] memory stakes) = middleware.getActiveOperatorsStakeAt(ts);
        assertEq(activeOperators.length, 1);
        assertEq(stakes.length, 1);
        assertEq(activeOperators[0], operator2);
        assertEq(stakes[0], stake2);
    }

    function test_stakeTooOldTimestamp() public {
        (address operator1,,,,,) = prepareTwoOperators();

        // Try to get stake for too old timestamp
        uint48 ts = uint48(vm.getBlockTimestamp());
        vm.warp(vm.getBlockTimestamp() + eraDuration * 2);
        vm.expectRevert(abi.encodeWithSelector(IMiddleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, ts);
    }

    function test_stakeCurrentTimestamp() public {
        (address operator1,,,,,) = prepareTwoOperators();

        // Try to get stake for current timestamp
        vm.expectRevert(abi.encodeWithSelector(IMiddleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, uint48(vm.getBlockTimestamp()));
    }

    function test_stakeFutureTimestamp() public {
        (address operator1,,,,,) = prepareTwoOperators();

        // Try to get stake for future timestamp
        vm.expectRevert(abi.encodeWithSelector(IMiddleware.IncorrectTimestamp.selector));
        middleware.getOperatorStakeAt(operator1, uint48(vm.getBlockTimestamp() + 1));
    }

    function test_slash() public {
        (address operator1,, address vault1,, uint256 stake1,) = prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);
        uint48 _vetoDeadline = vetoDeadline(IVault(vault1).slasher(), slashIndex);
        assertEq(_vetoDeadline, uint48(vm.getBlockTimestamp() + eraDuration / 2));

        // Try to execute slash before veto deadline
        vm.warp(_vetoDeadline - 1);
        vm.expectRevert(IVetoSlasher.VetoPeriodNotEnded.selector);
        executeSlash(vault1, slashIndex);

        // Execute slash when ready
        vm.warp(_vetoDeadline);
        executeSlash(vault1, slashIndex);

        // Check that operator1 stake is decreased
        vm.warp(_vetoDeadline + 1);
        assertEq(middleware.getOperatorStakeAt(operator1, _vetoDeadline), stake1 - 100);

        // Try to execute slash twice
        vm.expectRevert(IVetoSlasher.SlashRequestCompleted.selector);
        executeSlash(vault1, slashIndex);
    }

    function test_slashRequestUnknownOperator() public {
        (,, address vault1,,,) = prepareTwoOperators();

        // Try to request slash from unknown operator
        vm.warp(vm.getBlockTimestamp() + 1);
        requestSlash(
            address(0xdead), uint48(vm.getBlockTimestamp() - 1), vault1, 100, IMiddleware.NotRegisteredOperator.selector
        );
    }

    function test_slashRequestUnknownVault() public {
        (address operator1,,,,,) = prepareTwoOperators();

        // Try to request slash from unknown vault
        requestSlash(
            operator1, uint48(vm.getBlockTimestamp() - 1), address(0xdead), 100, IMiddleware.NotRegisteredVault.selector
        );
    }

    function test_slashRequestOnVaultWithNoStake() public {
        (address operator1,,, address vault2,,) = prepareTwoOperators();

        // Try to request slash on vault where it has no stake
        requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault2, 10, IVetoSlasher.InsufficientSlash.selector);
    }

    function test_slashAfterSlashPeriod() public {
        (address operator1,, address vault1,,,) = prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);

        // Try to slash after slash period
        vm.warp(uint48(vm.getBlockTimestamp()) + IVault(vault1).epochDuration());
        vm.expectRevert(IVetoSlasher.SlashPeriodEnded.selector);
        executeSlash(vault1, slashIndex);
    }

    function test_slashOneOperatorTwoVaults() public {
        (address operator1,, address vault1, address vault2,,) = prepareTwoOperators();

        // Try request slashes for one operator, but 2 vaults
        Middleware.VaultSlashData[] memory vaults = new Middleware.VaultSlashData[](2);
        vaults[0] = IMiddleware.VaultSlashData({vault: vault1, amount: 10});
        vaults[1] = IMiddleware.VaultSlashData({vault: vault2, amount: 20});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](1);
        slashes[0] =
            IMiddleware.SlashData({operator: operator1, ts: uint48(vm.getBlockTimestamp() - 1), vaults: vaults});

        requestSlash(slashes, IVetoSlasher.InsufficientSlash.selector);

        // Make one more vault for operator1
        address vault3 = createVaultForOperator(operator1);
        depositInto(vault3, 3_000);

        vm.warp(vm.getBlockTimestamp() + 1);

        // Request slashes with correct vaults
        vaults[1] = IMiddleware.VaultSlashData({vault: vault3, amount: 30});
        slashes[0] =
            IMiddleware.SlashData({operator: operator1, ts: uint48(vm.getBlockTimestamp() - 1), vaults: vaults});
        requestSlash(slashes, 0);
    }

    function test_slashTwoOperatorsTwoVaults() public {
        (address operator1, address operator2, address vault1, address vault2,,) = prepareTwoOperators();

        // Request slashes for 2 operators with corresponding vaults
        Middleware.VaultSlashData[] memory operator1Vaults = new Middleware.VaultSlashData[](1);
        operator1Vaults[0] = IMiddleware.VaultSlashData({vault: vault1, amount: 10});

        Middleware.VaultSlashData[] memory operator2Vaults = new Middleware.VaultSlashData[](1);
        operator2Vaults[0] = IMiddleware.VaultSlashData({vault: vault2, amount: 20});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](2);
        slashes[0] = IMiddleware.SlashData({
            operator: operator1, ts: uint48(vm.getBlockTimestamp() - 1), vaults: operator1Vaults
        });
        slashes[1] = IMiddleware.SlashData({
            operator: operator2, ts: uint48(vm.getBlockTimestamp() - 1), vaults: operator2Vaults
        });

        requestSlash(slashes, 0);
    }

    function test_slashVeto() public {
        (address operator1,, address vault1,,,) = prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);
        uint48 _vetoDeadline = vetoDeadline(IVault(vault1).slasher(), slashIndex);

        // Slash resolver is admin
        vm.startPrank(admin);
        {
            IVetoSlasher slasher = IVetoSlasher(IVault(vault1).slasher());

            // Try to execute slash after veto deadline
            vm.warp(_vetoDeadline);
            vm.expectRevert(IVetoSlasher.VetoPeriodEnded.selector);
            slasher.vetoSlash(slashIndex, new bytes(0));

            // Veto slash
            vm.warp(_vetoDeadline - 1);
            slasher.vetoSlash(slashIndex, new bytes(0));

            // Try to veto the slash after veto is done
            vm.expectRevert(IVetoSlasher.SlashRequestCompleted.selector);
            slasher.vetoSlash(slashIndex, new bytes(0));
        }
        vm.stopPrank();
    }

    function test_slashExecutionUnregisteredVault() public {
        (address operator1,, address vault1,,,) = prepareTwoOperators();

        // Make slash request for operator1 in vault1
        uint256 slashIndex = requestSlash(operator1, uint48(vm.getBlockTimestamp() - 1), vault1, 100, 0);

        // Try to execute slash for unknown vault
        vm.expectRevert(IMiddleware.NotRegisteredVault.selector);
        executeSlash(address(0xdead), slashIndex);
    }

    function prepareTwoOperators()
        private
        returns (address operator1, address operator2, address vault1, address vault2, uint256 stake1, uint256 stake2)
    {
        operator1 = address(0x1);
        operator2 = address(0x2);

        stake1 = 1_000;
        stake2 = 2_000;

        vault1 = createOperatorWithStake(operator1, stake1);
        vault2 = createOperatorWithStake(operator2, stake2);

        vm.warp(vm.getBlockTimestamp() + 1);
    }

    function vetoDeadline(address slasher, uint256 slashIndex) private view returns (uint48) {
        (,,,, uint48 _vetoDeadline,) = IVetoSlasher(slasher).slashRequests(slashIndex);
        return _vetoDeadline;
    }

    function executeSlash(address vault, uint256 index) private {
        Middleware.SlashIdentifier[] memory slashes = new Middleware.SlashIdentifier[](1);
        slashes[0] = IMiddleware.SlashIdentifier({vault: vault, index: index});

        vm.startPrank(admin);
        {
            middleware.executeSlash(slashes);
        }
        vm.stopPrank();
    }

    function requestSlash(address operator, uint48 ts, address vault, uint256 amount, bytes4 err)
        private
        returns (uint256 slashIndex)
    {
        Middleware.VaultSlashData[] memory vaults = new Middleware.VaultSlashData[](1);
        vaults[0] = IMiddleware.VaultSlashData({vault: vault, amount: amount});

        Middleware.SlashData[] memory slashes = new Middleware.SlashData[](1);
        slashes[0] = IMiddleware.SlashData({operator: operator, ts: ts, vaults: vaults});

        slashIndex = requestSlash(slashes, err)[0];
        assertNotEq(slashIndex, type(uint256).max);
    }

    function requestSlash(IMiddleware.SlashData[] memory slashes, bytes4 err)
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
            vm.startPrank(admin);
            {
                vm.expectRevert(err);
                middleware.requestSlash(slashes);
            }
            vm.stopPrank();

            return slashIndexes;
        } else {
            vm.startPrank(admin);
            {
                middleware.requestSlash(slashes);
            }
            vm.stopPrank();
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
}
