// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Gear} from "./libraries/Gear.sol";

interface IMiddleware {
    error UnknownVault();
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
    error NotRouter();
    error NotOperatorRewards();
    error UnknownStakerRewards();
    error InvalidStakerRewardsVault();

    /**
     * @notice ...
     * @param eraDuration ...
     */
    struct InitParams {
        uint48 eraDuration;
        uint48 minVaultEpochDuration;
        uint48 operatorGracePeriod;
        uint48 vaultGracePeriod;
        uint48 minVetoDuration;
        uint48 minSlashExecutionDelay;
        uint64 allowedVaultImplVersion;
        uint64 vetoSlasherImplType;
        uint256 maxResolverSetEpochsDelay;
        address vaultRegistry;
        address operatorRegistry;
        address networkRegistry;
        address networkOptIn;
        address middlewareService;
        address collateral;
        address roleSlashRequester;
        address roleSlashExecutor;
        address vetoResolver;

        address operatorRewards;
        address operatorRewardsFactory;
        address stakerRewardsFactory;
    }

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

    /**
     * @notice
     * @return
     */
    function ERA_DURATION() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function MIN_VAULT_EPOCH_DURATION() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function OPERATOR_GRACE_PERIOD() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function VAULT_GRACE_PERIOD() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function MIN_VETO_DURATION() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function MIN_SLASH_EXECUTION_DELAY() external view returns (uint48);

    /**
     * @notice
     * @return
     */
    function MAX_RESOLVER_SET_EPOCHS_DELAY() external view returns (uint256);

    /**
     * @notice
     * @return
     */
    function VAULT_REGISTRY() external view returns (address);

    /**
     * @notice
     * @return
     */
    function ALLOWED_VAULT_IMPL_VERSION() external view returns (uint64);

    /**
     * @notice
     * @return
     */
    function VETO_SLASHER_IMPL_TYPE() external view returns (uint64);

    /**
     * @notice
     * @return
     */
    function OPERATOR_REGISTRY() external view returns (address);

    /**
     * @notice
     * @return
     */
    function NETWORK_REGISTRY() external view returns (address);

    /**
     * @notice
     * @return
     */
    function NETWORK_OPT_IN() external view returns (address);

    /**
     * @notice
     * @return
     */
    function MIDDLEWARE_SERVICE() external view returns (address);

    /**
     * @notice
     * @return
     */
    function COLLATERAL() external view returns (address);

    /**
     * @notice
     * @return
     */
    function VETO_RESOLVER() external view returns (address);

    /**
     * @notice
     * @return
     */
    function SUBNETWORK() external view returns (bytes32);

    /**
     * @notice
     * @return
     */
    function OPERATOR_REWARDS() external view returns (address);

    /**
     * @notice
     * @return
     */
    function OPERATOR_REWARDS_FACTORY() external view returns (address);

    /**
     * @notice
     * @return
     */
    function STAKER_REWARDS_FACTORY() external view returns (address);

    /* Rewards distribution logic */

    /**
     * @notice Get a claimed amount of rewards for a particular account, network, and token.
     */
    function distributeOperatorRewards(Gear.OperatorRewardsCommitment memory _rewards) external;

    /**
     * @notice ...
     */
    function distributeStakerRewards(Gear.StakerRewardsCommitment memory _rewards) external;

    /* Other Functions */
    function changeSlashRequester(address newRole) external;

    function changeSlashExecutor(address newRole) external;

    // TODO: Check that total stake is big enough
    function registerOperator() external;

    function disableOperator() external;

    function enableOperator() external;

    function unregisterOperator(address operator) external;

    // TODO: check vault has enough stake
    function registerVault(address vault) external;

    function disableVault(address vault) external;

    function enableVault(address vault) external;

    function unregisterVault(address vault) external;

    function makeElectionAt(uint48 ts, uint256 maxValidators) external view returns (address[] memory);

    function getOperatorStakeAt(address operator, uint48 ts) external view returns (uint256 stake);

    function requestSlash(SlashData[] calldata data) external;

    function executeSlash(SlashIdentifier[] calldata slashes) external;
}
