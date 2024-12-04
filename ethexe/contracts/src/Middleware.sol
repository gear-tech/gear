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

// TODO (asap): document all functions and variables
// TODO (asap): implement rewards distribution
// TODO (asap): add validators commission
// TODO: introduce common struct for address and balance/value
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
// TODO: use hints for symbiotic calls
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
    error NotRegisteredVault();
    error NotRegisteredOperator();
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

    struct SlashIdentifier {
        address vault;
        uint256 index;
    }

    struct Config {
        uint48 eraDuration;
        uint48 minVaultEpochDuration;
        uint48 operatorGracePeriod;
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
    uint48 public immutable operatorGracePeriod;
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
    address public immutable vetoResolver;
    bytes32 public immutable subnetwork;

    address public roleSlashRequester;
    address public roleSlashExecutor;

    EnumerableMap.AddressToUintMap private operators;
    EnumerableMap.AddressToUintMap private vaults;

    constructor(Config memory cfg) {
        _validateConfiguration(cfg);

        eraDuration = cfg.eraDuration;
        minVaultEpochDuration = cfg.minVaultEpochDuration;
        operatorGracePeriod = cfg.operatorGracePeriod;
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

    function changeSlashRequester(address newRole) external _onlyRole(roleSlashRequester) {
        roleSlashRequester = newRole;
    }

    function changeSlashExecutor(address newRole) external _onlyRole(roleSlashExecutor) {
        roleSlashExecutor = newRole;
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

        if (disabledTime == 0 || Time.timestamp() < disabledTime + operatorGracePeriod) {
            revert OperatorGracePeriodNotPassed();
        }

        operators.remove(operator);
    }

    // TODO: check vault has enough stake
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

    function makeElectionAt(uint48 ts, uint256 maxValidators) public view returns (address[] memory) {
        require(maxValidators > 0, "Max validators must be greater than zero");

        (address[] memory activeOperators, uint256[] memory stakes) = getActiveOperatorsStakeAt(ts);

        if (activeOperators.length <= maxValidators) {
            return activeOperators;
        }

        // Bubble sort descending
        uint256 n = activeOperators.length;
        for (uint256 i = 0; i < n; i++) {
            for (uint256 j = 0; j < n - 1 - i; j++) {
                if (stakes[j] < stakes[j + 1]) {
                    (stakes[j], stakes[j + 1]) = (stakes[j + 1], stakes[j]);
                    (activeOperators[j], activeOperators[j + 1]) = (activeOperators[j + 1], activeOperators[j]);
                }
            }
        }

        // Choose between validators with the same stake
        uint256 sameStakeCount = 1;
        uint256 lastStake = stakes[maxValidators - 1];
        for (uint256 i = maxValidators; i < activeOperators.length; i++) {
            if (stakes[i] != lastStake) {
                break;
            }
            sameStakeCount += 1;
        }

        if (sameStakeCount > 1) {
            // If there are multiple validators with the same stake, choose one randomly
            uint256 randomIndex = uint256(keccak256(abi.encodePacked(ts))) % sameStakeCount;
            activeOperators[maxValidators - 1] = activeOperators[maxValidators + randomIndex - 1];
        }

        assembly {
            mstore(activeOperators, maxValidators)
        }

        return activeOperators;
    }

    function getOperatorStakeAt(address operator, uint48 ts) public view _validTimestamp(ts) returns (uint256 stake) {
        (uint48 enabledTime, uint48 disabledTime) = operators.getTimes(operator);
        if (!_wasActiveAt(enabledTime, disabledTime, ts)) {
            return 0;
        }

        stake = _collectOperatorStakeFromVaultsAt(operator, ts);
    }

    // TODO: change return signature
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

        assembly ("memory-safe") {
            mstore(activeOperators, operatorIdx)
            mstore(stakes, operatorIdx)
        }
    }

    function requestSlash(SlashData[] calldata data) external _onlyRole(roleSlashRequester) {
        for (uint256 i; i < data.length; ++i) {
            SlashData calldata slashData = data[i];
            if (!operators.contains(slashData.operator)) {
                revert NotRegisteredOperator();
            }

            for (uint256 j; j < slashData.vaults.length; ++j) {
                VaultSlashData calldata vaultData = slashData.vaults[j];

                if (!vaults.contains(vaultData.vault)) {
                    revert NotRegisteredVault();
                }

                address slasher = IVault(vaultData.vault).slasher();
                IVetoSlasher(slasher).requestSlash(
                    subnetwork, slashData.operator, vaultData.amount, slashData.ts, new bytes(0)
                );
            }
        }
    }

    function executeSlash(SlashIdentifier[] calldata slashes) external _onlyRole(roleSlashExecutor) {
        for (uint256 i; i < slashes.length; ++i) {
            SlashIdentifier calldata slash = slashes[i];

            if (!vaults.contains(slash.vault)) {
                revert NotRegisteredVault();
            }

            IVetoSlasher(IVault(slash.vault).slasher()).executeSlash(slash.index, new bytes(0));
        }
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
            cfg.operatorGracePeriod >= cfg.minVaultEpochDuration,
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

        // Symbiotic guarantees that any veto slasher has veto duration less than vault epoch duration.
        // But we also want to guarantee that there is some time to execute the slash.
        require(cfg.minSlashExecutionDelay > 0, "Min slash execution delay cannot be zero");
        require(
            cfg.minVetoDuration + cfg.minSlashExecutionDelay <= cfg.minVaultEpochDuration,
            "Veto duration and slash execution delay must be less than ot equal to min vaults epoch duration"
        );

        // In order to be able to change resolver, we need to limit max delay in epochs.
        // `3` - is minimal number of epochs, which is symbiotic veto slasher impl restrictions.
        require(cfg.maxResolverSetEpochsDelay >= 3, "Resolver set epochs delay must be at least 3");
    }

    // Timestamp must be always in the past, but not too far,
    // so that some operators or vaults can be already unregistered.
    modifier _validTimestamp(uint48 ts) {
        if (ts >= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        uint48 gracePeriod = operatorGracePeriod < vaultGracePeriod ? operatorGracePeriod : vaultGracePeriod;
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
