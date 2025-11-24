// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Gear} from "./libraries/Gear.sol";

import {IMiddleware} from "./IMiddleware.sol";
import {Subnetwork} from "symbiotic-core/src/contracts/libraries/Subnetwork.sol";

import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {
    ReentrancyGuardTransientUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";

contract POAMiddleware is IMiddleware, OwnableUpgradeable, ReentrancyGuardTransientUpgradeable {
    using EnumerableMap for EnumerableMap.AddressToUintMap;
    using MapWithTimeData for EnumerableMap.AddressToUintMap;

    using EnumerableMap for EnumerableMap.AddressToAddressMap;
    using MapWithTimeData for EnumerableMap.AddressToAddressMap;

    using Subnetwork for address;

    // keccak256(abi.encode(uint256(keccak256("middleware.storage.Slot")) - 1)) & ~bytes32(uint256(0xff));
    bytes32 private constant SLOT_STORAGE = 0x0b8c56af6cc9ad401ad225bfe96df77f3049ba17eadac1cb95ee89df1e69d100;

    bytes32 private constant DEFAULT_ADMIN_ROLE = 0x00;
    uint8 private constant NETWORK_IDENTIFIER = 0;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(InitParams calldata _params) public initializer {
        __Ownable_init(_params.owner);
        __ReentrancyGuardTransient_init();

        _setStorageSlot("middleware.storage.MiddlewareV1");
        Storage storage $ = _storage();

        $.eraDuration = _params.eraDuration;
        $.minVaultEpochDuration = _params.minVaultEpochDuration;
        $.operatorGracePeriod = _params.operatorGracePeriod;
        $.vaultGracePeriod = _params.vaultGracePeriod;
        $.minVetoDuration = _params.minVetoDuration;
        $.minSlashExecutionDelay = _params.minSlashExecutionDelay;
        $.maxResolverSetEpochsDelay = _params.maxResolverSetEpochsDelay;
        $.allowedVaultImplVersion = _params.allowedVaultImplVersion;
        $.vetoSlasherImplType = _params.vetoSlasherImplType;

        // TODO #4609
        $.collateral = _params.collateral;
        $.subnetwork = address(this).subnetwork(NETWORK_IDENTIFIER);
        $.maxAdminFee = _params.maxAdminFee;

        $.router = _params.router;

        $.symbiotic = _params.symbiotic;
    }

    /// @custom:oz-upgrades-validate-as-initializer
    function reinitialize() public onlyOwner reinitializer(2) {
        __Ownable_init(owner());

        Storage storage oldStorage = _storage();

        _setStorageSlot("middleware.storage.MiddlewareV2");
        Storage storage newStorage = _storage();

        newStorage.eraDuration = oldStorage.eraDuration;
        newStorage.minVaultEpochDuration = oldStorage.minVaultEpochDuration;
        newStorage.operatorGracePeriod = oldStorage.operatorGracePeriod;
        newStorage.vaultGracePeriod = oldStorage.vaultGracePeriod;
        newStorage.minVetoDuration = oldStorage.minVetoDuration;
        newStorage.minSlashExecutionDelay = oldStorage.minSlashExecutionDelay;
        newStorage.maxResolverSetEpochsDelay = oldStorage.maxResolverSetEpochsDelay;
        newStorage.allowedVaultImplVersion = oldStorage.allowedVaultImplVersion;
        newStorage.vetoSlasherImplType = oldStorage.vetoSlasherImplType;
        newStorage.collateral = oldStorage.collateral;
        newStorage.subnetwork = oldStorage.subnetwork;
        newStorage.maxAdminFee = oldStorage.maxAdminFee;
        newStorage.router = oldStorage.router;
        newStorage.symbiotic = oldStorage.symbiotic;

        for (uint256 i = 0; i < oldStorage.operators.length(); i++) {
            (address key, uint256 value) = oldStorage.operators.at(i);
            newStorage.operators.set(key, value);
        }

        for (uint256 i = 0; i < oldStorage.vaults.length(); i++) {
            (address key, uint256 value) = oldStorage.vaults.at(i);
            newStorage.vaults.set(key, value);
        }
    }

    // # Views
    function eraDuration() public view returns (uint48) {
        return _storage().eraDuration;
    }

    function minVaultEpochDuration() public view returns (uint48) {
        return _storage().minVaultEpochDuration;
    }

    function operatorGracePeriod() external view returns (uint48) {
        return _storage().operatorGracePeriod;
    }

    function vaultGracePeriod() external view returns (uint48) {
        return _storage().vaultGracePeriod;
    }

    function minVetoDuration() external view returns (uint48) {
        return _storage().minVetoDuration;
    }

    function minSlashExecutionDelay() external view returns (uint48) {
        return _storage().minSlashExecutionDelay;
    }

    function maxResolverSetEpochsDelay() external view returns (uint256) {
        return _storage().maxResolverSetEpochsDelay;
    }

    function allowedVaultImplVersion() external view returns (uint64) {
        return _storage().allowedVaultImplVersion;
    }

    function vetoSlasherImplType() external view returns (uint64) {
        return _storage().vetoSlasherImplType;
    }

    function collateral() external view returns (address) {
        return _storage().collateral;
    }

    function subnetwork() external view returns (bytes32) {
        return _storage().subnetwork;
    }

    function maxAdminFee() external view returns (uint256) {
        return _storage().maxAdminFee;
    }

    function router() external view returns (address) {
        return _storage().router;
    }

    function symbioticContracts() external view returns (Gear.SymbioticContracts memory) {
        return _storage().symbiotic;
    }

    // # POA Middleware allowed calls.

    function setValidators(address[] memory validators) external onlyOwner {
        Storage storage $ = _storage();
        for (uint256 i = 0; i < validators.length; i++) {
            $.operators.append(validators[i], 0);
        }
    }

    function disableOperator() external {
        _storage().operators.disable(msg.sender);
    }

    function enableOperator() external {
        _storage().operators.enable(msg.sender);
    }

    function makeElectionAt(uint48, uint256 maxValidators) external view returns (address[] memory) {
        require(maxValidators > 0, "Max validators must be greater than zero");

        Storage storage $ = _storage();
        address[] memory operators = new address[]($.operators.length());

        for (uint256 i; i < $.operators.length(); ++i) {
            (address operator,,) = $.operators.atWithTimes(i);
            operators[i] = operator;
        }

        if ($.operators.length() <= maxValidators) {
            return operators;
        }

        assembly ("memory-safe") {
            mstore(operators, maxValidators)
        }

        return operators;
    }

    // # Restricted POA Middleware calls.

    function changeSlashRequester(address) public pure {
        revert("Change Slash Requester not supported in Mock Middleware");
    }

    function changeSlashExecutor(address) public pure {
        revert("Change Slash Executor not supported in Mock Middleware");
    }

    function registerOperator() public pure {
        revert("Register validator by himself is not supported. See `setValidators` method.");
    }

    function unregisterOperator(address) public pure {
        revert("Unregister validator by himself is not supported.");
    }

    function distributeOperatorRewards(address, uint256, bytes32) public pure returns (bytes32) {
        revert("Rewards not supported in Mock Middleware");
    }

    function distributeStakerRewards(Gear.StakerRewardsCommitment memory, uint48) public pure returns (bytes32) {
        revert("Rewards not supported in Mock Middleware");
    }

    function registerVault(address, address) public pure {
        revert("Register Vault not supported, SYMBIOTIC not integrated yet");
    }

    function disableVault(address) public pure {
        revert("Disable Vault not supported, SYMBIOTIC not integrated yet");
    }

    function enableVault(address) public pure {
        revert("Enable Vault not supported, SYMBIOTIC not integrated yet");
    }

    function unregisterVault(address) public pure {
        revert("Unregister Vault not supported, SYMBIOTIC not integrated yet");
    }

    function getOperatorStakeAt(address, uint48) public pure returns (uint256) {
        revert("POA Middleware do not support stakes.");
    }

    function getActiveOperatorsStakeAt(uint48) public pure returns (address[] memory, uint256[] memory) {
        revert("POA Middleware do not support stakes.");
    }

    function requestSlash(SlashData[] calldata) public pure {
        revert("Request slash not supported, SYMBIOTIC not integrated yet");
    }

    function executeSlash(SlashIdentifier[] calldata) public pure {
        revert("Exectute slash not supported, SYMBIOTIC not integrated yet");
    }

    function _storage() private view returns (Storage storage middleware) {
        bytes32 slot = _getStorageSlot();

        assembly ("memory-safe") {
            middleware.slot := slot
        }
    }

    function _getStorageSlot() private view returns (bytes32) {
        return StorageSlot.getBytes32Slot(SLOT_STORAGE).value;
    }

    function _setStorageSlot(string memory namespace) private onlyOwner {
        bytes32 slot = keccak256(abi.encode(uint256(keccak256(bytes(namespace))) - 1)) & ~bytes32(uint256(0xff));
        StorageSlot.getBytes32Slot(SLOT_STORAGE).value = slot;
    }
}
