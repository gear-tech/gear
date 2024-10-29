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

contract Middleware {
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using MapWithTimeData for EnumerableMap.AddressToUintMap;
    using Subnetwork for address;

    error OperatorAlreadyRegistered();
    error ZeroVaultAddress();
    error NotKnownVault();
    error IncorrectDelegatorType();
    error VaultWrongEpochDuration();
    error UnknownCollateral();
    error NotEnoughStakeInVault();
    error OperatorGracePeriodNotPassed();
    error VaultGracePeriodNotPassed();
    error NotVaultOwner();
    error IncorrectTimestamp();
    error OperatorDoesNotExist();
    error OperatorDoesNotOptIn();

    uint48 public immutable ERA_DURATION;
    uint48 public immutable GENESIS_TIMESTAMP;
    address public immutable VAULT_FACTORY;
    address public immutable DELEGATOR_FACTORY;
    address public immutable SLASHER_FACTORY;
    address public immutable OPERATOR_REGISTRY;
    address public immutable NETWORK_OPT_IN;
    address public immutable COLLATERAL;

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
        VAULT_FACTORY = vaultFactory;
        DELEGATOR_FACTORY = delegatorFactory;
        SLASHER_FACTORY = slasherFactory;
        OPERATOR_REGISTRY = operatorRegistry;
        NETWORK_OPT_IN = networkOptIn;
        COLLATERAL = collateral;

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
        operators.append(msg.sender, address(0));
    }

    function disableOperator() external {
        operators.disable(msg.sender);
    }

    function enableOperator() external {
        operators.enable(msg.sender);
    }

    function unregisterOperator() external {
        (, uint48 disabledTime) = operators.getTimes(msg.sender);

        if (disabledTime == 0 || disabledTime + 2 * ERA_DURATION > Time.timestamp()) {
            revert OperatorGracePeriodNotPassed();
        }

        operators.remove(msg.sender);
    }

    // TODO: check vault has enough stake
    function registerVault(address vault) external {
        if (vault == address(0)) {
            revert ZeroVaultAddress();
        }

        if (!IRegistry(VAULT_FACTORY).isEntity(vault)) {
            revert NotKnownVault();
        }

        if (IVault(vault).epochDuration() < 2 * ERA_DURATION) {
            revert VaultWrongEpochDuration();
        }

        if (IVault(vault).collateral() != COLLATERAL) {
            revert UnknownCollateral();
        }

        IBaseDelegator(IVault(vault).delegator()).setMaxNetworkLimit(network_identifier(), type(uint256).max);

        vaults.append(vault, msg.sender);
    }

    function disableVault(address vault) external {
        address vault_owner = vaults.getPinnedAddress(vault);

        if (vault_owner != msg.sender) {
            revert NotVaultOwner();
        }

        vaults.disable(vault);
    }

    function enableVault(address vault) external {
        address vault_owner = vaults.getPinnedAddress(vault);

        if (vault_owner != msg.sender) {
            revert NotVaultOwner();
        }

        vaults.enable(vault);
    }

    function unregisterVault(address vault) external {
        (, uint48 disabledTime) = vaults.getTimes(vault);

        if (disabledTime == 0 || disabledTime + 2 * ERA_DURATION > Time.timestamp()) {
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

        for (uint256 i; i < vaults.length(); ++i) {
            (address vault, uint48 vaultEnabledTime, uint48 vaultDisabledTime) = vaults.atWithTimes(i);

            if (!_wasActiveAt(vaultEnabledTime, vaultDisabledTime, ts)) {
                continue;
            }

            stake += IBaseDelegator(IVault(vault).delegator()).stakeAt(subnetwork(), operator, ts, new bytes(0));
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

    function subnetwork() public view returns (bytes32) {
        return address(this).subnetwork(network_identifier());
    }

    function network_identifier() public pure returns (uint96) {
        return 0;
    }
}
