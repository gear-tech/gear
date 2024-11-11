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

// TODO: use camelCase for immutable variables
// TODO: document all functions and variables
// TODO: implement election logic
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
// TODO: implement rewards distribution
contract Middleware {
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using MapWithTimeData for EnumerableMap.AddressToUintMap;
    using Subnetwork for address;

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
    error VetoDurationTooLong();
    error IncompatibleVaultVersion();
    error NotRegistredVault();
    error NotRegistredOperator();
    error RoleMismatch();
    error ResolverMismatch();
    error ResolverSetDelayTooLong();

    struct VaultSlashData {
        address vault;
        uint256 amount;
    }

    struct SlashData {
        address operator;
        uint48 ts;
        VaultSlashData[] vaults;
    }

    struct Config {
        uint48 eraDuration;
        uint48 minVaultEpochDuration;
        uint48 operatoraGracePeriod;
        uint48 vaultGracePeriod;
        uint48 minVetoDuration;
        uint48 minSlashExecutionDelay;
        uint256 maxResolverSetEpochsDelay;
        address vaultRegistry;
        uint64 allowedVaultImplVersion;
        uint64 vetoSlasherImplType;
        address operatorRegistry;
        address networkRegistry;
        address networkOptIn;
        address middlewareService;
        address collateral;
        address roleSlashRequester;
        address roleSlashExecutor;
        address vetoResolver;
    }

    uint96 public constant NETWORK_IDENTIFIER = 0;

    uint48 public immutable eraDuration;
    uint48 public immutable minVaultEpochDuration;
    uint48 public immutable operatoraGracePeriod;
    uint48 public immutable vaultGracePeriod;
    uint48 public immutable minVetoDuration;
    uint48 public immutable minSlashExecutionDelay;
    uint256 public immutable maxResolverSetEpochsDelay;
    address public immutable vaultRegistry;
    uint64 public immutable allowedVaultImplVersion;
    uint64 public immutable vetoSlasherImplType;
    address public immutable operatorRegistry;
    address public immutable networkRegistry;
    address public immutable networkOptIn;
    address public immutable middlewareService;
    address public immutable collateral;
    address public immutable roleSlashRequester;
    address public immutable roleSlashExecutor;
    address public immutable vetoResolver;
    bytes32 public immutable subnetwork;

    EnumerableMap.AddressToUintMap private operators;
    EnumerableMap.AddressToUintMap private vaults;

    constructor(Config memory cfg) {
        _validateConfiguration(cfg);

        eraDuration = cfg.eraDuration;
        minVaultEpochDuration = cfg.minVaultEpochDuration;
        operatoraGracePeriod = cfg.operatoraGracePeriod;
        vaultGracePeriod = cfg.vaultGracePeriod;
        minVetoDuration = cfg.minVetoDuration;
        minSlashExecutionDelay = cfg.minSlashExecutionDelay;
        maxResolverSetEpochsDelay = cfg.maxResolverSetEpochsDelay;
        vaultRegistry = cfg.vaultRegistry;
        allowedVaultImplVersion = cfg.allowedVaultImplVersion;
        vetoSlasherImplType = cfg.vetoSlasherImplType;
        operatorRegistry = cfg.operatorRegistry;
        networkRegistry = cfg.networkRegistry;
        networkOptIn = cfg.networkOptIn;
        middlewareService = cfg.middlewareService;
        collateral = cfg.collateral;
        roleSlashRequester = cfg.roleSlashRequester;
        roleSlashExecutor = cfg.roleSlashExecutor;
        vetoResolver = cfg.vetoResolver;

        subnetwork = address(this).subnetwork(NETWORK_IDENTIFIER);

        // Presently network and middleware are the same address
        INetworkRegistry(networkRegistry).registerNetwork();
        INetworkMiddlewareService(middlewareService).setMiddleware(address(this));
    }

    // TODO: Check that total stake is big enough
    function registerOperator() external {
        if (!IRegistry(operatorRegistry).isEntity(msg.sender)) {
            revert OperatorDoesNotExist();
        }
        if (!IOptInService(networkOptIn).isOptedIn(msg.sender, address(this))) {
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

        if (disabledTime == 0 || Time.timestamp() < disabledTime + operatoraGracePeriod) {
            revert OperatorGracePeriodNotPassed();
        }

        operators.remove(operator);
    }

    // TODO: check vault has enough stake
    // TODO: support and check slasher
    // TODO: consider to use hints
    function registerVault(address vault) external {
        if (!IRegistry(vaultRegistry).isEntity(vault)) {
            revert NotKnownVault();
        }

        if (IMigratableEntity(vault).version() != allowedVaultImplVersion) {
            revert IncompatibleVaultVersion();
        }

        uint48 vaultEpochDuration = IVault(vault).epochDuration();
        if (vaultEpochDuration < minVaultEpochDuration) {
            revert VaultWrongEpochDuration();
        }

        if (IVault(vault).collateral() != collateral) {
            revert UnknownCollateral();
        }

        if (!IVault(vault).isDelegatorInitialized()) {
            revert DelegatorNotInitialized();
        }

        if (!IVault(vault).isSlasherInitialized()) {
            revert SlasherNotInitialized();
        }

        IBaseDelegator delegator = IBaseDelegator(IVault(vault).delegator());
        if (delegator.maxNetworkLimit(subnetwork) != type(uint256).max) {
            delegator.setMaxNetworkLimit(NETWORK_IDENTIFIER, type(uint256).max);
        }
        _delegatorHookCheck(IBaseDelegator(delegator).hook());

        address slasher = IVault(vault).slasher();
        if (IEntity(slasher).TYPE() != vetoSlasherImplType) {
            revert IncompatibleSlasherType();
        }
        if (IVetoSlasher(slasher).isBurnerHook()) {
            revert BurnerHookNotSupported();
        }
        uint48 vetoDuration = IVetoSlasher(slasher).vetoDuration();
        if (vetoDuration < minVetoDuration) {
            revert VetoDurationTooShort();
        }
        if (vetoDuration + minSlashExecutionDelay > vaultEpochDuration) {
            revert VetoDurationTooLong();
        }
        if (IVetoSlasher(slasher).resolverSetEpochsDelay() > maxResolverSetEpochsDelay) {
            revert ResolverSetDelayTooLong();
        }

        address resolver = IVetoSlasher(slasher).resolver(subnetwork, new bytes(0));
        if (resolver == address(0)) {
            IVetoSlasher(slasher).setResolver(NETWORK_IDENTIFIER, vetoResolver, new bytes(0));
        } else if (resolver != vetoResolver) {
            // TODO: consider how to support this case
            revert ResolverMismatch();
        }

        _burnerCheck(IVault(vault).burner());

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

        if (disabledTime == 0 || Time.timestamp() < disabledTime + vaultGracePeriod) {
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

    // TODO: consider to use hints
    function requestSlash(SlashData[] calldata data) external _onlyRole(roleSlashRequester) {
        for (uint256 i; i < data.length; ++i) {
            SlashData calldata slash_data = data[i];
            if (!operators.contains(slash_data.operator)) {
                revert NotRegistredOperator();
            }

            for (uint256 j; j < slash_data.vaults.length; ++j) {
                VaultSlashData calldata vault_data = slash_data.vaults[j];

                if (!vaults.contains(vault_data.vault)) {
                    revert NotRegistredVault();
                }

                address slasher = IVault(vault_data.vault).slasher();
                IVetoSlasher(slasher).requestSlash(
                    subnetwork, slash_data.operator, vault_data.amount, slash_data.ts, new bytes(0)
                );
            }
        }
    }

    // TODO: consider to use hints
    // TODO: array of slashes
    function executeSlash(address vault, uint256 index) external _onlyRole(roleSlashExecutor) {
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

            stake += IBaseDelegator(IVault(vault).delegator()).stakeAt(subnetwork, operator, ts, new bytes(0));
        }
    }

    function _wasActiveAt(uint48 enabledTime, uint48 disabledTime, uint48 ts) private pure returns (bool) {
        return enabledTime != 0 && enabledTime <= ts && (disabledTime == 0 || disabledTime >= ts);
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

    function _validateConfiguration(Config memory cfg) private pure {
        require(cfg.eraDuration > 0, "Era duration cannot be zero");

        // Middleware must support cases when election for next era is made before the start of the next era,
        // so the min vaults epoch duration must be bigger than `eraDuration + electionDelay`.
        // The election delay is less than or equal to the era duration, so limit `2 * eraDuration` is enough.
        require(
            cfg.minVaultEpochDuration >= 2 * cfg.eraDuration, "Min vaults epoch duration must be bigger than 2 eras"
        );

        // Operator grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require(
            cfg.operatoraGracePeriod >= cfg.minVaultEpochDuration,
            "Operator grace period must be bigger than min vaults epoch duration"
        );

        // Vault grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require(
            cfg.vaultGracePeriod >= cfg.minVaultEpochDuration,
            "Vault grace period must be bigger than min vaults epoch duration"
        );

        // Give some time for the resolvers to veto slashes.
        require(cfg.minVetoDuration > 0, "Veto duration cannot be zero");

        // Simbiotic guarantees that any veto slasher has veto duration less than vault epoch duration.
        // But we also want to guaratie that there is some time to execute the slash.
        require(cfg.minSlashExecutionDelay > 0, "Min slash execution delay cannot be zero");
        require(
            cfg.minVetoDuration + cfg.minSlashExecutionDelay <= cfg.minVaultEpochDuration,
            "Veto duration and slash execution delay must be less than ot equal to min vaults epoch duration"
        );

        // In order to be able to change resolver, we need to limit max delay in epochs.
        // `3` - is minimal number of epochs, which is simbiotic veto slasher impl restrictions.
        require(cfg.maxResolverSetEpochsDelay >= 3, "Resolver set epochs delay must be at least 3");
    }

    // Timestamp must be always in the past, but not too far,
    // so that some operators or vaults can be already unregistered.
    modifier _validTimestamp(uint48 ts) {
        if (ts >= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        uint48 gracePeriod = operatoraGracePeriod < vaultGracePeriod ? operatoraGracePeriod : vaultGracePeriod;
        if (ts + gracePeriod <= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        _;
    }

    modifier _onlyRole(address role) {
        if (msg.sender != role) {
            revert RoleMismatch();
        }
        _;
    }
}
