// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {IMiddleware} from "./IMiddleware.sol";
import {Gear} from "./libraries/Gear.sol";
import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {
    ReentrancyGuardTransientUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Subnetwork} from "symbiotic-core/src/contracts/libraries/Subnetwork.sol";

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

    // The operators stored in middleware.
    address[] private operators;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(InitParams calldata _params) public initializer {
        __Ownable_init(_params.owner);
        __ReentrancyGuardTransient_init();

        _setStorageSlot("middleware.storage.MiddlewareV1");
        Storage storage $ = _storage();

        $.router = _params.router;
    }

    /// @custom:oz-upgrades-validate-as-initializer
    function reinitialize() public onlyOwner reinitializer(2) {
        __Ownable_init(owner());

        Storage storage oldStorage = _storage();

        _setStorageSlot("middleware.storage.MiddlewareV2");
        Storage storage newStorage = _storage();

        newStorage.router = oldStorage.router;
    }

    // # POA Middleware allowed calls.
    function setValidators(address[] memory validators) external onlyOwner {
        operators = validators;
    }

    function makeElectionAt(uint48, uint256) external view returns (address[] memory) {
        return operators;
    }

    function router() external view returns (address) {
        return _storage().router;
    }

    ///////////////////////////////////////////
    //         NOT IMPLEMENTED CALLS
    ///////////////////////////////////////////

    // # Views
    function eraDuration() public pure returns (uint48) {
        revert("not implemented");
    }

    function minVaultEpochDuration() public pure returns (uint48) {
        revert("not implemented");
    }

    function operatorGracePeriod() public pure returns (uint48) {
        revert("not implemented");
    }

    function vaultGracePeriod() public pure returns (uint48) {
        revert("not implemented");
    }

    function minVetoDuration() public pure returns (uint48) {
        revert("not implemented");
    }

    function minSlashExecutionDelay() public pure returns (uint48) {
        revert("not implemented");
    }

    function maxResolverSetEpochsDelay() public pure returns (uint256) {
        revert("not implemented");
    }

    function allowedVaultImplVersion() public pure returns (uint64) {
        revert("not implemented");
    }

    function vetoSlasherImplType() public pure returns (uint64) {
        revert("not implemented");
    }

    function collateral() public pure returns (address) {
        revert("not implemented");
    }

    function subnetwork() public pure returns (bytes32) {
        revert("not implemented");
    }

    function maxAdminFee() public pure returns (uint256) {
        revert("not implemented");
    }

    function symbioticContracts() public pure returns (Gear.SymbioticContracts memory) {
        revert("not implemented");
    }

    function disableOperator() public pure {
        revert("not implemented");
    }

    function enableOperator() public pure {
        revert("not implemented");
    }

    function changeSlashRequester(address) public pure {
        revert("not implemented");
    }

    function changeSlashExecutor(address) public pure {
        revert("not implemented");
    }

    function registerOperator() public pure {
        revert("not implemented");
    }

    function unregisterOperator(address) public pure {
        revert("not implemented");
    }

    function distributeOperatorRewards(address, uint256, bytes32) public pure returns (bytes32) {
        revert("not implemented");
    }

    function distributeStakerRewards(Gear.StakerRewardsCommitment memory, uint48) public pure returns (bytes32) {
        revert("not implemented");
    }

    function registerVault(address, address) public pure {
        revert("not implemented");
    }

    function disableVault(address) public pure {
        revert("not implemented");
    }

    function enableVault(address) public pure {
        revert("not implemented");
    }

    function unregisterVault(address) public pure {
        revert("not implemented");
    }

    function getOperatorStakeAt(address, uint48) public pure returns (uint256) {
        revert("not implemented");
    }

    function getActiveOperatorsStakeAt(uint48) public pure returns (address[] memory, uint256[] memory) {
        revert("not implemented");
    }

    function requestSlash(SlashData[] calldata) public pure {
        revert("not implemented");
    }

    function executeSlash(SlashIdentifier[] calldata) public pure {
        revert("not implemented");
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
