// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.26;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";

import {Subnetwork} from "symbiotic-core/src/contracts/libraries/Subnetwork.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IRegistry} from "symbiotic-core/src/interfaces/common/IRegistry.sol";
import {IEntity} from "symbiotic-core/src/interfaces/common/IEntity.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {INetworkRegistry} from "symbiotic-core/src/interfaces/INetworkRegistry.sol";
import {IOptInService} from "symbiotic-core/src/interfaces/service/IOptInService.sol";

import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";

// TODO: support slashing
// TODO: implement election logic
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
contract Middleware {
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using MapWithTimeData for EnumerableMap.AddressToUintMap;
    using Subnetwork for address;

    error ZeroVaultAddress();
    error NotKnownVault();
    error VaultWrongEpochDuration();
    error UnknownCollateral();
    error OperatorGracePeriodNotPassed();
    error VaultGracePeriodNotPassed();
    error NotVaultOwner();
    error IncorrectTimestamp();
    error OperatorDoesNotExist();
    error OperatorDoesNotOptIn();

    uint48 public immutable ERA_DURATION;
    uint48 public immutable GENESIS_TIMESTAMP;
    uint48 public immutable OPERATOR_GRACE_PERIOD;
    uint48 public immutable VAULT_GRACE_PERIOD;
    uint48 public immutable VAULT_MIN_EPOCH_DURATION;
    address public immutable VAULT_FACTORY;
    address public immutable DELEGATOR_FACTORY;
    address public immutable SLASHER_FACTORY;
    address public immutable OPERATOR_REGISTRY;
    address public immutable NETWORK_OPT_IN;
    address public immutable COLLATERAL;
    bytes32 public immutable SUBNETWORK;
    uint96 public immutable NETWORK_IDENTIFIER = 0;

    EnumerableMap.AddressToUintMap private operators;
    EnumerableMap.AddressToUintMap private vaults;

    constructor(
        uint48 eraDuration,
        address vaultFactory,
        address delegatorFactory,
        address slasherFactory,
        address operatorRegistry,
        address networkRegistry,
        address networkOptIn,
        address collateral
    ) {
        ERA_DURATION = eraDuration;
        GENESIS_TIMESTAMP = Time.timestamp();
        OPERATOR_GRACE_PERIOD = 2 * eraDuration;
        VAULT_GRACE_PERIOD = 2 * eraDuration;
        VAULT_MIN_EPOCH_DURATION = 2 * eraDuration;
        VAULT_FACTORY = vaultFactory;
        DELEGATOR_FACTORY = delegatorFactory;
        SLASHER_FACTORY = slasherFactory;
        OPERATOR_REGISTRY = operatorRegistry;
        NETWORK_OPT_IN = networkOptIn;
        COLLATERAL = collateral;
        SUBNETWORK = address(this).subnetwork(NETWORK_IDENTIFIER);

        INetworkRegistry(networkRegistry).registerNetwork();
    }

    // TODO: append total operator stake check is big enough
    // TODO: append check operator is opt-in network
    // TODO: append check operator is operator registry entity
    function registerOperator() external {
        if (!IRegistry(OPERATOR_REGISTRY).isEntity(msg.sender)) {
            revert OperatorDoesNotExist();
        }
        if (!IOptInService(NETWORK_OPT_IN).isOptedIn(msg.sender, address(this))) {
            revert OperatorDoesNotOptIn();
        }
        operators.append(msg.sender, 0);
    }

    function disableOperator() external {
        operators.disable(msg.sender);
    }

    function enableOperator() external {
        operators.enable(msg.sender);
    }

    function unregisterOperator(address operator) external {
        (, uint48 disabledTime) = operators.getTimes(operator);

        if (disabledTime == 0 || disabledTime + OPERATOR_GRACE_PERIOD > Time.timestamp()) {
            revert OperatorGracePeriodNotPassed();
        }

        operators.remove(operator);
    }

    // TODO: check vault has enough stake
    // TODO: support and check slasher
    function registerVault(address vault) external {
        if (vault == address(0)) {
            revert ZeroVaultAddress();
        }

        if (!IRegistry(VAULT_FACTORY).isEntity(vault)) {
            revert NotKnownVault();
        }

        if (IVault(vault).epochDuration() < VAULT_MIN_EPOCH_DURATION) {
            revert VaultWrongEpochDuration();
        }

        if (IVault(vault).collateral() != COLLATERAL) {
            revert UnknownCollateral();
        }

        address delegator = IVault(vault).delegator();
        if (IBaseDelegator(delegator).maxNetworkLimit(SUBNETWORK) != type(uint256).max) {
            IBaseDelegator(delegator).setMaxNetworkLimit(NETWORK_IDENTIFIER, type(uint256).max);
        }

        vaults.append(vault, uint160(msg.sender));
    }

    function disableVault(address vault) external {
        address vault_owner = address(vaults.getPinnedData(vault));

        if (vault_owner != msg.sender) {
            revert NotVaultOwner();
        }

        vaults.disable(vault);
    }

    function enableVault(address vault) external {
        address vault_owner = address(vaults.getPinnedData(vault));

        if (vault_owner != msg.sender) {
            revert NotVaultOwner();
        }

        vaults.enable(vault);
    }

    function unregisterVault(address vault) external {
        (, uint48 disabledTime) = vaults.getTimes(vault);

        if (disabledTime == 0 || disabledTime + VAULT_GRACE_PERIOD > Time.timestamp()) {
            revert VaultGracePeriodNotPassed();
        }

        vaults.remove(vault);
    }

    function getOperatorStakeAt(address operator, uint48 ts) external view returns (uint256 stake) {
        _checkTimestampInThePast(ts);

        (uint48 enabledTime, uint48 disabledTime) = operators.getTimes(operator);
        if (!_wasActiveAt(enabledTime, disabledTime, ts)) {
            return 0;
        }

        stake = _collectOperatorStakeFromVaultsAt(operator, ts);
    }

    function getActiveOperatorsStakeAt(uint48 ts)
        public
        view
        returns (address[] memory active_operators, uint256[] memory stakes)
    {
        _checkTimestampInThePast(ts);

        active_operators = new address[](operators.length());
        stakes = new uint256[](operators.length());

        uint256 operatorIdx = 0;

        for (uint256 i; i < operators.length(); ++i) {
            (address operator, uint48 enabled, uint48 disabled) = operators.atWithTimes(i);

            if (!_wasActiveAt(enabled, disabled, ts)) {
                continue;
            }

            active_operators[operatorIdx] = operator;
            stakes[operatorIdx] = _collectOperatorStakeFromVaultsAt(operator, ts);
            operatorIdx += 1;
        }

        assembly {
            mstore(active_operators, operatorIdx)
            mstore(stakes, operatorIdx)
        }
    }

    function _collectOperatorStakeFromVaultsAt(address operator, uint48 ts) private view returns (uint256 stake) {
        for (uint256 i; i < vaults.length(); ++i) {
            (address vault, uint48 vaultEnabledTime, uint48 vaultDisabledTime) = vaults.atWithTimes(i);

            if (!_wasActiveAt(vaultEnabledTime, vaultDisabledTime, ts)) {
                continue;
            }

            stake += IBaseDelegator(IVault(vault).delegator()).stakeAt(SUBNETWORK, operator, ts, new bytes(0));
        }
    }

    function _wasActiveAt(uint48 enabledTime, uint48 disabledTime, uint48 ts) private pure returns (bool) {
        return enabledTime != 0 && enabledTime <= ts && (disabledTime == 0 || disabledTime >= ts);
    }

    // Timestamp must be always in the past
    function _checkTimestampInThePast(uint48 ts) private view {
        if (ts >= Time.timestamp()) {
            revert IncorrectTimestamp();
        }
    }
}
