// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.26;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";

import {Subnetwork} from "symbiotic-core/src/contracts/libraries/Subnetwork.sol";
import {IVault} from "symbiotic-core/src/interfaces/vault/IVault.sol";
import {IRegistry} from "symbiotic-core/src/interfaces/common/IRegistry.sol";
import {IEntity} from "symbiotic-core/src/interfaces/common/IEntity.sol";
import {IBaseDelegator} from "symbiotic-core/src/interfaces/delegator/IBaseDelegator.sol";
import {INetworkRegistry} from "symbiotic-core/src/interfaces/INetworkRegistry.sol";
import {IOptInService} from "symbiotic-core/src/interfaces/service/IOptInService.sol";
import {INetworkMiddlewareService} from "symbiotic-core/src/interfaces/service/INetworkMiddlewareService.sol";
import {IVetoSlasher} from "symbiotic-core/src/interfaces/slasher/IVetoSlasher.sol";
import {IMigratableEntity} from "symbiotic-core/src/interfaces/common/IMigratableEntity.sol";

import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";

// TODO: support slashing
// TODO: use camelCase for immutable variables
// TODO: implement election logic
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
// TODO: implement rewards distribution
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
    error UnsupportedHook();
    error UnsupportedBurner();
    error DelegatorNotInitialized();
    error SlasherNotInitialized();
    error IncompatibleSlasherType();
    error BurnerHookNotSupported();
    error VetoDurationTooShort();
    error IncompatibleVaultVersion();
    error NotRegistredVault();
    error NotRegistredOperator();

    struct VaultSlashData {
        address vault;
        uint256 amount;
    }

    struct SlashData {
        address operator;
        uint48 ts;
        VaultSlashData[] vaults;
    }

    struct MiddlewareConfig {
        uint48 eraDuration;
        uint48 minVetoDuration;
        address vaultRegistry;
        uint64 allowedVaultImplVersion;
        uint64 vetoSlasherImplType;
        address operatorRegistry;
        address networkRegistry;
        address networkOptIn;
        address middlewareService;
        address collateral;
    }

    uint96 public constant NETWORK_IDENTIFIER = 0;
    uint256 public constant MAX_RESOLVER_SET_EPOCHS_DELAY = 10;

    uint96 public constant NETWORK_IDENTIFIER = 0;

    uint48 public immutable ERA_DURATION;
    uint48 public immutable MIN_VETO_DURATION;
    uint48 public immutable GENESIS_TIMESTAMP;
    uint48 public immutable OPERATOR_GRACE_PERIOD;
    uint48 public immutable VAULT_GRACE_PERIOD;
    uint48 public immutable VAULT_MIN_EPOCH_DURATION;

    address public immutable VAULT_REGISTRY;
    uint64 public immutable ALLOWED_VAULT_IMPL_VERSION;
    uint64 public immutable VETO_SLASHER_IMPL_TYPE;
    address public immutable OPERATOR_REGISTRY;
    address public immutable NETWORK_OPT_IN;
    address public immutable COLLATERAL;
    bytes32 public immutable SUBNETWORK;

    EnumerableMap.AddressToUintMap private operators;
    EnumerableMap.AddressToUintMap private vaults;

    constructor(MiddlewareConfig memory cfg) {
        ERA_DURATION = cfg.eraDuration;
        MIN_VETO_DURATION = cfg.minVetoDuration;
        GENESIS_TIMESTAMP = Time.timestamp();
        OPERATOR_GRACE_PERIOD = 2 * ERA_DURATION;
        VAULT_GRACE_PERIOD = 2 * ERA_DURATION;
        VAULT_MIN_EPOCH_DURATION = 2 * ERA_DURATION;
        VAULT_REGISTRY = cfg.vaultRegistry;
        ALLOWED_VAULT_IMPL_VERSION = cfg.allowedVaultImplVersion;
        VETO_SLASHER_IMPL_TYPE = cfg.vetoSlasherImplType;
        OPERATOR_REGISTRY = cfg.operatorRegistry;
        NETWORK_OPT_IN = cfg.networkOptIn;
        COLLATERAL = cfg.collateral;
        SUBNETWORK = address(this).subnetwork(NETWORK_IDENTIFIER);

        // Presently network and middleware are the same address
        INetworkRegistry(cfg.networkRegistry).registerNetwork();
        INetworkMiddlewareService(cfg.middlewareService).setMiddleware(address(this));
    }

    // TODO: Check that total stake is big enough
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

        if (disabledTime == 0 || Time.timestamp() < disabledTime + OPERATOR_GRACE_PERIOD) {
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

        if (!IRegistry(VAULT_REGISTRY).isEntity(vault)) {
            revert NotKnownVault();
        }

        if (IMigratableEntity(vault).version() != ALLOWED_VAULT_IMPL_VERSION) {
            revert IncompatibleVaultVersion();
        }

        if (IVault(vault).epochDuration() < VAULT_MIN_EPOCH_DURATION) {
            revert VaultWrongEpochDuration();
        }

        if (IVault(vault).collateral() != COLLATERAL) {
            revert UnknownCollateral();
        }

        if (!IVault(vault).isDelegatorInitialized()) {
            revert DelegatorNotInitialized();
        }
        IBaseDelegator delegator = IBaseDelegator(IVault(vault).delegator());
        if (delegator.maxNetworkLimit(SUBNETWORK) != type(uint256).max) {
            delegator.setMaxNetworkLimit(NETWORK_IDENTIFIER, type(uint256).max);
        }
        _delegatorHookCheck(IBaseDelegator(delegator).hook());

        if (!IVault(vault).isSlasherInitialized()) {
            revert SlasherNotInitialized();
        }
        address slasher = IVault(vault).slasher();
        if (IEntity(slasher).TYPE() != VETO_SLASHER_IMPL_TYPE) {
            revert IncompatibleSlasherType();
        }
        if (IVetoSlasher(slasher).isBurnerHook()) {
            revert BurnerHookNotSupported();
        }
        if (IVetoSlasher(slasher).vetoDuration() < MIN_VETO_DURATION) {
            revert VetoDurationTooShort();
        }

        _burnerCheck(IVault(vault).burner());

        IVetoSlasher(slasher).setResolver(NETWORK_IDENTIFIER, address(this), new bytes(0));

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

        if (disabledTime == 0 || Time.timestamp() < disabledTime + VAULT_GRACE_PERIOD) {
            revert VaultGracePeriodNotPassed();
        }

        vaults.remove(vault);
    }

    // TODO: consider to append ability to use hints
    function getOperatorStakeAt(address operator, uint48 ts)
        external
        view
        _validTimestamp(ts)
        returns (uint256 stake)
    {
        (uint48 enabledTime, uint48 disabledTime) = operators.getTimes(operator);
        if (!_wasActiveAt(enabledTime, disabledTime, ts)) {
            return 0;
        }

        stake = _collectOperatorStakeFromVaultsAt(operator, ts);
    }

    // TODO: consider to append ability to use hints
    function getActiveOperatorsStakeAt(uint48 ts)
        public
        view
        _validTimestamp(ts)
        returns (address[] memory activeOperators, uint256[] memory stakes)
    {
        activeOperators = new address[](operators.length());
        stakes = new uint256[](operators.length());

        uint256 operatorIdx = 0;

        for (uint256 i; i < operators.length(); ++i) {
            (address operator, uint48 enabled, uint48 disabled) = operators.atWithTimes(i);

            if (!_wasActiveAt(enabled, disabled, ts)) {
                continue;
            }

            activeOperators[operatorIdx] = operator;
            stakes[operatorIdx] = _collectOperatorStakeFromVaultsAt(operator, ts);
            operatorIdx += 1;
        }

        assembly {
            mstore(activeOperators, operatorIdx)
            mstore(stakes, operatorIdx)
        }
    }

    // TODO: Only router can call this function
    // TODO: consider to use hints
    function requestSlash(SlashData[] calldata data) external {
        for (uint256 i; i < data.length; ++i) {
            SlashData calldata slash_data = data[i];
            if (!operators.contains(slash_data.operator)) {
                revert NotRegistredOperator();
            }

            for (uint256 j; j < data.length; ++j) {
                VaultSlashData calldata vault_data = slash_data.vaults[j];

                if (!vaults.contains(vault_data.vault)) {
                    revert NotRegistredVault();
                }

                address slasher = IVault(vault_data.vault).slasher();
                IVetoSlasher(slasher).requestSlash(
                    SUBNETWORK, slash_data.operator, vault_data.amount, slash_data.ts, new bytes(0)
                );
            }
        }
    }

    // TODO: only slashes executor
    function executeSlash(address vault, uint256 index) external {
        if (!vaults.contains(vault)) {
            revert NotRegistredVault();
        }

        address slasher = IVault(vault).slasher();
        IVetoSlasher(slasher).executeSlash(index, new bytes(0));
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

    // Timestamp must be always in the past, but not too far,
    // so that some operators or vaults can be already unregistered.
    modifier _validTimestamp(uint48 ts) {
        if (ts >= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        uint48 gracePeriod = OPERATOR_GRACE_PERIOD < VAULT_GRACE_PERIOD ? OPERATOR_GRACE_PERIOD : VAULT_GRACE_PERIOD;
        if (ts + gracePeriod <= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        _;
    }

    // Supports only null hook for now
    function _delegatorHookCheck(address hook) private pure {
        if (hook != address(0)) {
            revert UnsupportedHook();
        }
    }

    // Supports only null burner for now
    function _burnerCheck(address burner) private pure {
        if (burner == address(0)) {
            revert UnsupportedBurner();
        }
    }
}
