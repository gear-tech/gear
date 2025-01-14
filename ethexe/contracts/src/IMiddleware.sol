// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

interface IMiddleware {
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

    function changeSlashRequester(address newRole) external;

    function changeSlashExecutor(address newRole) external;

    function registerOperator() external;

    function disableOperator() external;

    function enableOperator() external;

    function unregisterOperator(address operator) external;

    function registerVault(address vault) external;

    function disableVault(address vault) external;

    function enableVault(address vault) external;

    function unregisterVault(address vault) external;

    function makeElectionAt(uint48 ts, uint256 maxValidators) external view returns (address[] memory);

    function getOperatorStakeAt(address operator, uint48 ts) external view returns (uint256);

    function getActiveOperatorsStakeAt(uint48 ts)
        external
        view
        returns (address[] memory activeOperators, uint256[] memory stakes);

    function requestSlash(SlashData[] calldata data) external;

    function executeSlash(SlashIdentifier[] calldata slashes) external;
}
