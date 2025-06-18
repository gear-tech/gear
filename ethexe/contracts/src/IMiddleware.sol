// SPDX-License-Identifier: MIT
pragma solidity ^0.8.28;

import {Gear} from "./libraries/Gear.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";

/// @title Gear.exe Middleware Interface
/// @notice The Middleware contract is responsible for managing the interaction between the Router (Gear.exe) and the Symbiotic Ecosystem.
/// @dev The Middleware contract is designed to reduce the complexity of the Router contract.
interface IMiddleware {
    // # Errors.

    /// @dev Emitted when trying to register the vault from unknown factory.
    error NonFactoryVault();
    /// @dev Emitted when trying to register the vault with `epochDuration` less than `minVaultEpochDuration`.
    error VaultWrongEpochDuration();
    /// @dev Emitted when trying to distribute rewards with collateral that is not equal to the one in the Middleware.
    error UnknownCollateral();
    /// @dev Emitted when trying to unregister the operator earlier then `operatorGracePeriod`.
    error OperatorGracePeriodNotPassed();
    /// @dev Emitted when trying to unregister the vault earlier then `vaultGracePeriod`.
    error VaultGracePeriodNotPassed();
    /// @dev Emitted when `msg.sender` is no the owner.
    error NotVaultOwner();
    /// @dev Emitted when requested timestamp is in the future.
    error IncorrectTimestamp();
    /// @dev Emitted when the operator is not registered in the OperatorRegistry.
    error OperatorDoesNotExist();
    /// @dev Emitted when the operator is not opted-in to the Middleware.
    error OperatorDoesNotOptIn();
    /// @dev Emitted when the delegator's hook is not equal to `address(0)`.
    error UnsupportedDelegatorHook();
    /// @dev Emitted when vault's burner is equal to `address(0)`.
    error UnsupportedBurner();
    /// @dev Emitted in `registerVault` when vault's delegator is not initialized.
    error DelegatorNotInitialized();
    /// @dev Emitted in `registerVault` when vault's slasher is not initialized.
    error SlasherNotInitialized();
    /// @dev Emitted in `registerVault` when the vaults' slasher type is not supported.
    error IncompatibleSlasherType();
    /// @dev Emitted when vault's slasher has a burner hook.
    error BurnerHookNotSupported();
    /// @dev Emitted when the vault's slasher has a `vetoDuration` less than `minVetoDuration`.
    error VetoDurationTooShort();
    /// @dev Emitted when the vault's slasher has a `vetoDuration` + `minShashExecutionDelay` is greater than vaultEpochDuration.
    error VetoDurationTooLong();
    /// @dev Emitted when the vault has incompatible version.
    /// @notice The version of the vault is a index of the whitelisted versions in VaultFactory.
    error IncompatibleVaultVersion();
    /// @dev Emitted when rewards contract has incompatible version.
    /// @notice The version of the rewards contract is a index of the whitelisted versions in StakerRewardsFactory.
    error IncompatibleStakerRewardsVersion();
    /// @dev Emitted when the vault is not registered in the Middleware.
    error NotRegisteredVault();
    /// @dev Emitted when `SlashData` contains the operator that is not registered in the Middleware.
    error NotRegisteredOperator();
    /// @dev Emitted when slasher's veto resolver is not the same as in the Middleware.
    error ResolverMismatch();
    /// @dev Emitted when the slasher's delay to update the resolver is greater than the one in the Middleware.
    error ResolverSetDelayTooLong();
    /// @dev Emitted when the `msg.sender` is not the Router contract.
    error NotRouter();
    /// @dev Emitted when the `msg.sender` has not the role of slash requester.
    error NotSlashRequester();
    /// @dev Emitted when the `msg.sender` has not the role of slash excutor.
    error NotSlashExecutor();
    /// @dev Emitted when rewards contract was not created by the StakerRewardsFactory.
    error NonFactoryStakerRewards();
    /// @dev Emitted in `registerVault` when the vault in rewards contract is not the same as in the function parameter.
    error InvalidStakerRewardsVault();

    struct InitParams {
        address owner;
        uint48 eraDuration;
        uint48 minVaultEpochDuration;
        uint48 operatorGracePeriod;
        uint48 vaultGracePeriod;
        uint48 minVetoDuration;
        uint48 minSlashExecutionDelay;
        uint64 allowedVaultImplVersion;
        uint64 vetoSlasherImplType;
        uint256 maxResolverSetEpochsDelay;
        address collateral;
        uint256 maxAdminFee;
        address operatorRewards;
        address router;
        address roleSlashRequester;
        address roleSlashExecutor;
        address vetoResolver;
        Gear.SymbioticRegistries registries;
    }

    /// @custom:storage-location erc7201:middleware.storage.Middleware.
    struct Storage {
        uint48 eraDuration;
        uint48 minVaultEpochDuration;
        uint48 operatorGracePeriod;
        uint48 vaultGracePeriod;
        uint48 minVetoDuration;
        uint48 minSlashExecutionDelay;
        uint256 maxResolverSetEpochsDelay;
        uint64 allowedVaultImplVersion;
        uint64 vetoSlasherImplType;
        address collateral;
        bytes32 subnetwork;
        uint256 maxAdminFee;
        address operatorRewards;
        address router;
        address roleSlashRequester;
        address roleSlashExecutor;
        address vetoResolver;
        /// @notice Stores the addresses for Symbiotic Ecosystem contracts.
        /// @dev These addresses was taken from official documentation (https://docs.symbiotic.fi/deployments/mainnet).
        Gear.SymbioticRegistries registries;
        EnumerableMap.AddressToUintMap operators;
        EnumerableMap.AddressToUintMap vaults;
        mapping(address => Gear.AggregatedPublicKey) operatorPublicKeys;
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

    // # Views.
    function eraDuration() external view returns (uint48);
    function minVaultEpochDuration() external view returns (uint48);
    function operatorGracePeriod() external view returns (uint48);
    function vaultGracePeriod() external view returns (uint48);
    function minVetoDuration() external view returns (uint48);
    function minSlashExecutionDelay() external view returns (uint48);
    function maxResolverSetEpochsDelay() external view returns (uint256);
    function allowedVaultImplVersion() external view returns (uint64);
    function vetoSlasherImplType() external view returns (uint64);
    function collateral() external view returns (address);
    function subnetwork() external view returns (bytes32);
    function maxAdminFee() external view returns (uint256);
    function operatorRewards() external view returns (address);
    function router() external view returns (address);
    function roleSlashRequester() external view returns (address);
    function roleSlashExecutor() external view returns (address);
    function operatorRegistry() external view returns (address);
    function operatorPublicKeys(address operator) external view returns (uint256, uint256);

    // # Calls.
    function changeSlashRequester(address newRole) external;

    function changeSlashExecutor(address newRole) external;

    /// @dev This function returns the list of validators that are will be responsible for block production in the next era.
    function makeElectionAt(uint48 ts, uint256 maxValidators) external view returns (address[] memory);

    /// @return stake The total stake of the operator in all vaults that was active at the given timestamp.
    function getOperatorStakeAt(address operator, uint48 ts) external view returns (uint256 stake);

    function requestSlash(SlashData[] calldata data) external;

    function executeSlash(SlashIdentifier[] calldata slashes) external;

    /* Operators managing */

    /// @notice This function can be called only be operator themselves.
    /// @dev Operator must be registered in operator registry.
    function registerOperator() external;

    /// @notice This function can be called only be operator themselves.
    function disableOperator() external;

    /// @notice This function can be called only be operator themselves.
    function enableOperator() external;

    /// @notice This function can be called only be operator themselves.
    function unregisterOperator(address operator) external;

    /// @notice Registers a public key for an operator
    /// @param publicKey The aggregated public key to register
    function registerPublicKey(Gear.AggregatedPublicKey calldata publicKey) external;

    /* Vaults managing */

    /// @notice This function can be called only by the vault owner.
    function registerVault(address vault, address rewards) external;

    /// @notice This function can be called only by the vault owner.
    function unregisterVault(address vault) external;

    /// @notice This function can be called only by the vault owner.
    function disableVault(address vault) external;

    /// @notice This function can be called only by the vault owner.
    function enableVault(address vault) external;

    /* Rewards distribution */

    /// @notice The function can be called only by the Router contract.
    function distributeOperatorRewards(address token, uint256 amount, bytes32 root) external returns (bytes32);

    /// @notice The function can be called only by the Router contract.
    function distributeStakerRewards(Gear.StakerRewardsCommitment memory _rewards, uint48 timestamp)
        external
        returns (bytes32);
}
