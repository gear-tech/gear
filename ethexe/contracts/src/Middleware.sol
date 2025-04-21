// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {Gear} from "./libraries/Gear.sol";

import {IMiddleware} from "./IMiddleware.sol";
import {IRouter} from "./IRouter.sol";
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
import {IDefaultOperatorRewards} from
    "symbiotic-rewards/src/interfaces/defaultOperatorRewards/IDefaultOperatorRewards.sol";
import {IDefaultOperatorRewardsFactory} from
    "symbiotic-rewards/src/interfaces/defaultOperatorRewards/IDefaultOperatorRewardsFactory.sol";
import {IDefaultStakerRewards} from "symbiotic-rewards/src/interfaces/defaultStakerRewards/IDefaultStakerRewards.sol";
import {IDefaultStakerRewardsFactory} from
    "symbiotic-rewards/src/interfaces/defaultStakerRewards/IDefaultStakerRewardsFactory.sol";

import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";
import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";

// TODO (asap): document all functions and variables
// TODO (asap): add validators commission
// TODO: introduce common struct for address and balance/value
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
// TODO: use hints for symbiotic calls
// TODO: implement migreatable or upgreadable logic
contract Middleware is IMiddleware {
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using MapWithTimeData for EnumerableMap.AddressToUintMap;

    using EnumerableMap for EnumerableMap.AddressToAddressMap;
    using MapWithTimeData for EnumerableMap.AddressToAddressMap;

    using Subnetwork for address;

    uint96 public constant NETWORK_IDENTIFIER = 0;
    uint48 public immutable ERA_DURATION;
    uint48 public immutable MIN_VAULT_EPOCH_DURATION;
    uint48 public immutable OPERATOR_GRACE_PERIOD;
    uint48 public immutable VAULT_GRACE_PERIOD;
    uint48 public immutable MIN_VETO_DURATION;
    uint48 public immutable MIN_SLASH_EXECUTION_DELAY;
    uint64 public immutable ALLOWED_VAULT_IMPL_VERSION;
    uint64 public immutable VETO_SLASHER_IMPL_TYPE;
    uint256 public immutable MAX_RESOLVER_SET_EPOCHS_DELAY;

    address public immutable VAULT_REGISTRY;
    address public immutable OPERATOR_REGISTRY;
    address public immutable NETWORK_REGISTRY;
    address public immutable NETWORK_OPT_IN;
    address public immutable MIDDLEWARE_SERVICE;

    // TODO: think about multiple assets as collateral
    address public immutable COLLATERAL;
    address public immutable VETO_RESOLVER;
    bytes32 public immutable SUBNETWORK;

    address public immutable OPERATOR_REWARDS;
    address public immutable OPERATOR_REWARDS_FACTORY;
    address public immutable STAKER_REWARDS_FACTORY;

    // TODO: calculate better commission for admin fee
    uint256 public MAX_ADMIN_FEE = 1000;

    address public immutable ROUTER;

    address public roleSlashRequester;
    address public roleSlashExecutor;

    bytes32 public constant DEFAULT_ADMIN_ROLE = 0x00;

    EnumerableMap.AddressToUintMap private operators;

    // vault -> (enableTime, disableTime, rewards)
    EnumerableMap.AddressToUintMap private vaults;

    constructor(InitParams memory params) {
        _validateInitParams(params);

        ROUTER = msg.sender;

        ERA_DURATION = params.eraDuration;
        MIN_VAULT_EPOCH_DURATION = params.minVaultEpochDuration;
        OPERATOR_GRACE_PERIOD = params.operatorGracePeriod;
        VAULT_GRACE_PERIOD = params.vaultGracePeriod;
        MIN_VETO_DURATION = params.minVetoDuration;
        MIN_SLASH_EXECUTION_DELAY = params.minSlashExecutionDelay;
        MAX_RESOLVER_SET_EPOCHS_DELAY = params.maxResolverSetEpochsDelay;
        VAULT_REGISTRY = params.vaultRegistry;
        ALLOWED_VAULT_IMPL_VERSION = params.allowedVaultImplVersion;
        VETO_SLASHER_IMPL_TYPE = params.vetoSlasherImplType;
        OPERATOR_REGISTRY = params.operatorRegistry;
        NETWORK_REGISTRY = params.networkRegistry;
        NETWORK_OPT_IN = params.networkOptIn;
        MIDDLEWARE_SERVICE = params.middlewareService;
        COLLATERAL = params.collateral;
        VETO_RESOLVER = params.vetoResolver;

        roleSlashRequester = params.roleSlashRequester;
        roleSlashExecutor = params.roleSlashExecutor;

        SUBNETWORK = address(this).subnetwork(NETWORK_IDENTIFIER);

        OPERATOR_REWARDS = params.operatorRewards;
        OPERATOR_REWARDS_FACTORY = params.operatorRewardsFactory;
        STAKER_REWARDS_FACTORY = params.stakerRewardsFactory;

        // Presently network and middleware are the same address
        INetworkRegistry(NETWORK_REGISTRY).registerNetwork();
        INetworkMiddlewareService(MIDDLEWARE_SERVICE).setMiddleware(address(this));
    }

    function changeSlashRequester(address newRole) external _onlyRole(roleSlashRequester) {
        roleSlashRequester = newRole;
    }

    function changeSlashExecutor(address newRole) external _onlyRole(roleSlashExecutor) {
        roleSlashExecutor = newRole;
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

    function distributeOperatorRewards(Gear.OperatorRewardsCommitment memory _rewards) external _onlyRouter {
        if (_rewards.token != COLLATERAL) {
            revert UnknownCollateral();
        }

        IDefaultOperatorRewards(OPERATOR_REWARDS).distributeRewards(
            ROUTER, _rewards.token, _rewards.amount, _rewards.root
        );
    }

    function distributeStakerRewards(Gear.StakerRewardsCommitment memory _commitment) external _onlyRouter {
        for (uint256 i = 0; i < _commitment.distribution.length; ++i) {
            Gear.StakerRewards memory stakerRewards = _commitment.distribution[i];

            if (stakerRewards.token != COLLATERAL) {
                revert UnknownCollateral();
            }

            if (!vaults.contains(stakerRewards.vault)) {
                revert UnknownVault();
            }

            address rewards = address(vaults.getPinnedData(stakerRewards.vault));

            // TODO: consider to add hints instead of bytes("")
            bytes memory data = abi.encode(_commitment.timestamp, MAX_ADMIN_FEE, bytes(""), bytes(""));
            IDefaultStakerRewards(rewards).distributeRewards(ROUTER, stakerRewards.token, stakerRewards.amount, data);
        }
    }

    function registerVault(address _vault, address _rewards) external _vaultOwner(_vault) {
        _validateVault(_vault);
        _validateStakerRewards(_vault, _rewards);

        vaults.append(_vault, uint160(_rewards));
    }

    function disableVault(address vault) external _vaultOwner(vault) {
        vaults.disable(vault);
    }

    function enableVault(address vault) external _vaultOwner(vault) {
        vaults.enable(vault);
    }

    function unregisterVault(address vault) external _vaultOwner(vault) {
        (, uint48 disabledTime) = vaults.getTimes(vault);

        if (disabledTime == 0 || Time.timestamp() < disabledTime + VAULT_GRACE_PERIOD) {
            revert VaultGracePeriodNotPassed();
        }

        vaults.remove(vault);
    }

    function makeElectionAt(uint48 ts, uint256 maxValidators) external view returns (address[] memory) {
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

        assembly ("memory-safe") {
            mstore(activeOperators, maxValidators)
        }

        return activeOperators;
    }

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
                    SUBNETWORK, slashData.operator, vaultData.amount, slashData.ts, new bytes(0)
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

            stake += IBaseDelegator(IVault(vault).delegator()).stakeAt(SUBNETWORK, operator, ts, new bytes(0));
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

    function _validateInitParams(InitParams memory params) private pure {
        require(params.eraDuration > 0, "Era duration cannot be zero");

        // Middleware must support cases when election for next era is made before the start of the next era,
        // so the min vaults epoch duration must be bigger than `eraDuration + electionDelay`.
        // The election delay is less than or equal to the era duration, so limit `2 * eraDuration` is enough.
        require(
            params.minVaultEpochDuration >= 2 * params.eraDuration,
            "Min vaults epoch duration must be bigger than 2 eras"
        );

        // Operator grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require(
            params.operatorGracePeriod >= params.minVaultEpochDuration,
            "Operator grace period must be bigger than min vaults epoch duration"
        );

        // Vault grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require(
            params.vaultGracePeriod >= params.minVaultEpochDuration,
            "Vault grace period must be bigger than min vaults epoch duration"
        );

        // Give some time for the resolvers to veto slashes.
        require(params.minVetoDuration > 0, "Veto duration cannot be zero");

        // Symbiotic guarantees that any veto slasher has veto duration less than vault epoch duration.
        // But we also want to guarantee that there is some time to execute the slash.
        require(params.minSlashExecutionDelay > 0, "Min slash execution delay cannot be zero");
        require(
            params.minVetoDuration + params.minSlashExecutionDelay <= params.minVaultEpochDuration,
            "Veto duration and slash execution delay must be less than or equal to min vaults epoch duration"
        );

        // In order to be able to change resolver, we need to limit max delay in epochs.
        // `3` - is minimal number of epochs, which is symbiotic veto slasher impl restrictions.
        require(params.maxResolverSetEpochsDelay >= 3, "Resolver set epochs delay must be at least 3");
    }

    // TODO: check vault has enough stake
    function _validateVault(address _vault) private {
        if (!IRegistry(VAULT_REGISTRY).isEntity(_vault)) {
            revert NonFactoryVault();
        }

        if (IMigratableEntity(_vault).version() != ALLOWED_VAULT_IMPL_VERSION) {
            revert IncompatibleVaultVersion();
        }

        if (IVault(_vault).collateral() != COLLATERAL) {
            revert UnknownCollateral();
        }

        /* Checking time */
        uint48 vaultEpochDuration = IVault(_vault).epochDuration();
        if (vaultEpochDuration < MIN_VAULT_EPOCH_DURATION) {
            revert VaultWrongEpochDuration();
        }

        /* Validate delegator */
        if (!IVault(_vault).isDelegatorInitialized()) {
            revert DelegatorNotInitialized();
        }

        IBaseDelegator delegator = IBaseDelegator(IVault(_vault).delegator());
        if (delegator.maxNetworkLimit(SUBNETWORK) != type(uint256).max) {
            delegator.setMaxNetworkLimit(NETWORK_IDENTIFIER, type(uint256).max);
        }
        _delegatorHookCheck(IBaseDelegator(delegator).hook());

        /* Validate Slasher */
        if (!IVault(_vault).isSlasherInitialized()) {
            revert SlasherNotInitialized();
        }

        address slasher = IVault(_vault).slasher();
        if (IEntity(slasher).TYPE() != VETO_SLASHER_IMPL_TYPE) {
            revert IncompatibleSlasherType();
        }

        if (IVetoSlasher(slasher).isBurnerHook()) {
            revert BurnerHookNotSupported();
        }

        uint48 vetoDuration = IVetoSlasher(slasher).vetoDuration();
        if (vetoDuration < MIN_VETO_DURATION) {
            revert VetoDurationTooShort();
        }

        if (vetoDuration + MIN_SLASH_EXECUTION_DELAY > vaultEpochDuration) {
            revert VetoDurationTooLong();
        }

        if (IVetoSlasher(slasher).resolverSetEpochsDelay() > MAX_RESOLVER_SET_EPOCHS_DELAY) {
            revert ResolverSetDelayTooLong();
        }

        address resolver = IVetoSlasher(slasher).resolver(SUBNETWORK, new bytes(0));
        if (resolver == address(0)) {
            IVetoSlasher(slasher).setResolver(NETWORK_IDENTIFIER, VETO_RESOLVER, new bytes(0));
        } else if (resolver != VETO_RESOLVER) {
            // TODO: consider how to support this case
            revert ResolverMismatch();
        }

        // TODO: consider allow transfer burned funds to ROUTER address
        if (IVault(_vault).burner() == address(0)) {
            revert UnsupportedBurner();
        }
    }

    function _validateStakerRewards(address _vault, address _rewards) private view {
        if (!IRegistry(STAKER_REWARDS_FACTORY).isEntity(_rewards)) {
            revert UnknownStakerRewards();
        }

        if (IDefaultStakerRewards(_rewards).VAULT() != _vault) {
            revert InvalidStakerRewardsVault();
        }

        if (IDefaultStakerRewards(_rewards).version() != 1) {
            revert IncompatibleStakerRewardsVersion();
        }
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

    // TODO: consider to remove this
    modifier _onlyRole(address role) {
        if (msg.sender != role) {
            revert RoleMismatch();
        }
        _;
    }

    modifier _vaultOwner(address vault) {
        if (!IAccessControl(vault).hasRole(DEFAULT_ADMIN_ROLE, msg.sender)) {
            revert NotVaultOwner();
        }
        _;
    }

    modifier _onlyRouter() {
        if (msg.sender != ROUTER) {
            revert NotRouter();
        }
        _;
    }
}
