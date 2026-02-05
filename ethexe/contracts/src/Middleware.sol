// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {Gear} from "./libraries/Gear.sol";

import {IMiddleware} from "./IMiddleware.sol";
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
import {
    IDefaultOperatorRewards
} from "symbiotic-rewards/src/interfaces/defaultOperatorRewards/IDefaultOperatorRewards.sol";
import {IDefaultStakerRewards} from "symbiotic-rewards/src/interfaces/defaultStakerRewards/IDefaultStakerRewards.sol";

import {MapWithTimeData} from "./libraries/MapWithTimeData.sol";
import {IAccessControl} from "@openzeppelin/contracts/access/IAccessControl.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {
    ReentrancyGuardTransientUpgradeable
} from "@openzeppelin/contracts-upgradeable/utils/ReentrancyGuardTransientUpgradeable.sol";
import {StorageSlot} from "@openzeppelin/contracts/utils/StorageSlot.sol";

// TODO (asap): document all functions and variables
// TODO (asap): add validators commission
// TODO: introduce common struct for address and balance/value
// TODO: implement forced operators removal
// TODO: implement forced vaults removal
// TODO: use hints for symbiotic calls
contract Middleware is IMiddleware, OwnableUpgradeable, ReentrancyGuardTransientUpgradeable {
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

        INetworkRegistry(_params.symbiotic.networkRegistry).registerNetwork();
        INetworkMiddlewareService(_params.symbiotic.middlewareService).setMiddleware(address(this));

        _validateStorage($);
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

    // # Calls.

    function changeSlashRequester(address newRole) external {
        Storage storage $ = _storage();
        if (msg.sender != $.symbiotic.roleSlashRequester) {
            revert NotSlashRequester();
        }
        $.symbiotic.roleSlashRequester = newRole;
    }

    function changeSlashExecutor(address newRole) external {
        Storage storage $ = _storage();
        if (msg.sender != $.symbiotic.roleSlashExecutor) {
            revert NotSlashExecutor();
        }
        $.symbiotic.roleSlashExecutor = newRole;
    }

    // TODO: Check that total stake is big enough
    function registerOperator() external {
        Storage storage $ = _storage();

        if (!IRegistry($.symbiotic.operatorRegistry).isEntity(msg.sender)) {
            revert OperatorDoesNotExist();
        }
        if (!IOptInService($.symbiotic.networkOptIn).isOptedIn(msg.sender, address(this))) {
            revert OperatorDoesNotOptIn();
        }

        $.operators.append(msg.sender, 0);
    }

    function disableOperator() external {
        _storage().operators.disable(msg.sender);
    }

    function enableOperator() external {
        _storage().operators.enable(msg.sender);
    }

    function unregisterOperator(address operator) external {
        Storage storage $ = _storage();

        (, uint48 disabledTime) = $.operators.getTimes(operator);

        if (disabledTime == 0 || Time.timestamp() < disabledTime + $.operatorGracePeriod) {
            revert OperatorGracePeriodNotPassed();
        }

        $.operators.remove(operator);
    }

    function distributeOperatorRewards(address token, uint256 amount, bytes32 root) external returns (bytes32) {
        Storage storage $ = _storage();

        if (msg.sender != $.router) {
            revert NotRouter();
        }

        if (token != $.collateral) {
            revert UnknownCollateral();
        }

        IDefaultOperatorRewards($.symbiotic.operatorRewards).distributeRewards($.router, token, amount, root);

        return keccak256(abi.encodePacked(amount, root));
    }

    function distributeStakerRewards(Gear.StakerRewardsCommitment memory _commitment, uint48 timestamp)
        external
        returns (bytes32)
    {
        Storage storage $ = _storage();

        if (msg.sender != $.router) {
            revert NotRouter();
        }

        if (_commitment.token != $.collateral) {
            revert UnknownCollateral();
        }

        bytes memory distributionBytes;
        for (uint256 i = 0; i < _commitment.distribution.length; ++i) {
            Gear.StakerRewards memory rewards = _commitment.distribution[i];

            if (!$.vaults.contains(rewards.vault)) {
                revert NotRegisteredVault();
            }

            address rewardsAddress = address($.vaults.getPinnedData(rewards.vault));

            bytes memory data = abi.encode(timestamp, $.maxAdminFee, bytes(""), bytes(""));
            IDefaultStakerRewards(rewardsAddress).distributeRewards($.router, _commitment.token, rewards.amount, data);

            distributionBytes = bytes.concat(distributionBytes, abi.encodePacked(rewards.vault, rewards.amount));
        }

        return keccak256(bytes.concat(distributionBytes, abi.encodePacked(_commitment.totalAmount, _commitment.token)));
    }

    function registerVault(address _vault, address _rewards) external vaultOwner(_vault) {
        _validateVault(_vault);
        _validateStakerRewards(_vault, _rewards);

        _storage().vaults.append(_vault, uint160(_rewards));
    }

    function disableVault(address vault) external vaultOwner(vault) {
        _storage().vaults.disable(vault);
    }

    function enableVault(address vault) external vaultOwner(vault) {
        _storage().vaults.enable(vault);
    }

    function unregisterVault(address vault) external vaultOwner(vault) {
        Storage storage $ = _storage();
        (, uint48 disabledTime) = $.vaults.getTimes(vault);

        if (disabledTime == 0 || Time.timestamp() < disabledTime + $.vaultGracePeriod) {
            revert VaultGracePeriodNotPassed();
        }

        $.vaults.remove(vault);
    }

    function makeElectionAt(uint48 ts, uint256 maxValidators) external view returns (address[] memory) {
        require(maxValidators > 0, MaxValidatorsMustBeGreaterThanZero());

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

    function getOperatorStakeAt(address operator, uint48 ts) external view validTimestamp(ts) returns (uint256 stake) {
        (uint48 enabledTime, uint48 disabledTime) = _storage().operators.getTimes(operator);
        if (!_wasActiveAt(enabledTime, disabledTime, ts)) {
            return 0;
        }

        stake = _collectOperatorStakeFromVaultsAt(operator, ts);
    }

    // TODO: change return signature
    function getActiveOperatorsStakeAt(uint48 ts)
        public
        view
        validTimestamp(ts)
        returns (address[] memory activeOperators, uint256[] memory stakes)
    {
        Storage storage $ = _storage();
        activeOperators = new address[]($.operators.length());
        stakes = new uint256[]($.operators.length());

        uint256 operatorIdx = 0;

        for (uint256 i; i < $.operators.length(); ++i) {
            (address operator, uint48 enabled, uint48 disabled) = $.operators.atWithTimes(i);

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

    function requestSlash(SlashData[] calldata data) external {
        Storage storage $ = _storage();

        if (msg.sender != $.symbiotic.roleSlashRequester) {
            revert NotSlashRequester();
        }

        for (uint256 i; i < data.length; ++i) {
            SlashData calldata slashData = data[i];
            if (!$.operators.contains(slashData.operator)) {
                revert NotRegisteredOperator();
            }

            for (uint256 j; j < slashData.vaults.length; ++j) {
                VaultSlashData calldata vaultData = slashData.vaults[j];

                if (!$.vaults.contains(vaultData.vault)) {
                    revert NotRegisteredVault();
                }

                address slasher = IVault(vaultData.vault).slasher();
                IVetoSlasher(slasher)
                    .requestSlash($.subnetwork, slashData.operator, vaultData.amount, slashData.ts, new bytes(0));
            }
        }
    }

    function executeSlash(SlashIdentifier[] calldata slashes) external {
        if (msg.sender != _storage().symbiotic.roleSlashExecutor) {
            revert NotSlashExecutor();
        }

        for (uint256 i; i < slashes.length; ++i) {
            SlashIdentifier calldata slash = slashes[i];

            if (!_storage().vaults.contains(slash.vault)) {
                revert NotRegisteredVault();
            }

            IVetoSlasher(IVault(slash.vault).slasher()).executeSlash(slash.index, new bytes(0));
        }
    }

    function _collectOperatorStakeFromVaultsAt(address operator, uint48 ts) private view returns (uint256 stake) {
        Storage storage $ = _storage();
        for (uint256 i; i < $.vaults.length(); ++i) {
            (address vault, uint48 vaultEnabledTime, uint48 vaultDisabledTime) = $.vaults.atWithTimes(i);

            if (!_wasActiveAt(vaultEnabledTime, vaultDisabledTime, ts)) {
                continue;
            }

            stake += IBaseDelegator(IVault(vault).delegator()).stakeAt($.subnetwork, operator, ts, new bytes(0));
        }
    }

    function _wasActiveAt(uint48 enabledTime, uint48 disabledTime, uint48 ts) private pure returns (bool) {
        return enabledTime != 0 && enabledTime <= ts && (disabledTime == 0 || disabledTime >= ts);
    }

    // Supports only null hook for now
    function _delegatorHookCheck(address hook) private pure {
        if (hook != address(0)) {
            revert UnsupportedDelegatorHook();
        }
    }

    function _validateStorage(Storage storage $) private view {
        require($.eraDuration > 0, EraDurationMustBeGreaterThanZero());

        // Middleware must support cases when election for next era is made before the start of the next era,
        // so the min vaults epoch duration must be bigger than `eraDuration + electionDelay`.
        // The election delay is less than or equal to the era duration, so limit `2 * eraDuration` is enough.
        require($.minVaultEpochDuration >= 2 * $.eraDuration, MinVaultEpochDurationLessThanTwoEras());

        // Operator grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require($.operatorGracePeriod >= $.minVaultEpochDuration, OperatorGracePeriodLessThanMinVaultEpochDuration());

        // Vault grace period cannot be smaller than minimum vaults epoch duration.
        // Otherwise, it would be impossible to do slash in the next era sometimes.
        require($.vaultGracePeriod >= $.minVaultEpochDuration, VaultGracePeriodLessThanMinVaultEpochDuration());

        // Give some time for the resolvers to veto slashes.
        require($.minVetoDuration > 0, MinVetoDurationMustBeGreaterThanZero());

        // Symbiotic guarantees that any veto slasher has veto duration less than vault epoch duration.
        // But we also want to guarantee that there is some time to execute the slash.
        require($.minSlashExecutionDelay > 0, MinSlashExecutionDelayMustBeGreaterThanZero());
        require(
            $.minVetoDuration + $.minSlashExecutionDelay <= $.minVaultEpochDuration,
            MinVetoAndSlashDelayTooLongForVaultEpoch()
        );

        // In order to be able to change resolver, we need to limit max delay in epochs.
        // `3` - is minimal number of epochs, which is symbiotic veto slasher impl restrictions.
        require($.maxResolverSetEpochsDelay >= 3, ResolverSetDelayMustBeAtLeastThree());
    }

    // TODO: check vault has enough stake
    function _validateVault(address _vault) private {
        Storage storage $ = _storage();

        if (!IRegistry($.symbiotic.vaultRegistry).isEntity(_vault)) {
            revert NonFactoryVault();
        }

        if (IMigratableEntity(_vault).version() != $.allowedVaultImplVersion) {
            revert IncompatibleVaultVersion();
        }

        if (IVault(_vault).collateral() != $.collateral) {
            revert UnknownCollateral();
        }

        /* Checking time */
        uint48 vaultEpochDuration = IVault(_vault).epochDuration();
        if (vaultEpochDuration < $.minVaultEpochDuration) {
            revert VaultWrongEpochDuration();
        }

        /* Validate delegator */
        if (!IVault(_vault).isDelegatorInitialized()) {
            revert DelegatorNotInitialized();
        }

        IBaseDelegator delegator = IBaseDelegator(IVault(_vault).delegator());
        if (delegator.maxNetworkLimit($.subnetwork) != type(uint256).max) {
            delegator.setMaxNetworkLimit(NETWORK_IDENTIFIER, type(uint256).max);
        }
        _delegatorHookCheck(IBaseDelegator(delegator).hook());

        /* Validate Slasher */
        if (!IVault(_vault).isSlasherInitialized()) {
            revert SlasherNotInitialized();
        }

        address slasher = IVault(_vault).slasher();
        if (IEntity(slasher).TYPE() != $.vetoSlasherImplType) {
            revert IncompatibleSlasherType();
        }

        if (IVetoSlasher(slasher).isBurnerHook()) {
            revert BurnerHookNotSupported();
        }

        uint48 vetoDuration = IVetoSlasher(slasher).vetoDuration();
        if (vetoDuration < $.minVetoDuration) {
            revert VetoDurationTooShort();
        }

        if (vetoDuration + $.minSlashExecutionDelay > vaultEpochDuration) {
            revert VetoDurationTooLong();
        }

        if (IVetoSlasher(slasher).resolverSetEpochsDelay() > $.maxResolverSetEpochsDelay) {
            revert ResolverSetDelayTooLong();
        }

        address resolver = IVetoSlasher(slasher).resolver($.subnetwork, new bytes(0));
        if (resolver == address(0)) {
            IVetoSlasher(slasher).setResolver(NETWORK_IDENTIFIER, $.symbiotic.vetoResolver, new bytes(0));
        } else if (resolver != $.symbiotic.vetoResolver) {
            // TODO: consider how to support this case
            revert ResolverMismatch();
        }

        // TODO: consider allow transfer burned funds to ROUTER address
        if (IVault(_vault).burner() == address(0)) {
            revert UnsupportedBurner();
        }
    }

    function _validateStakerRewards(address _vault, address _rewards) private view {
        if (!IRegistry(_storage().symbiotic.stakerRewardsFactory).isEntity(_rewards)) {
            revert NonFactoryStakerRewards();
        }

        if (IDefaultStakerRewards(_rewards).VAULT() != _vault) {
            revert InvalidStakerRewardsVault();
        }

        if (IDefaultStakerRewards(_rewards).version() != 2) {
            revert IncompatibleStakerRewardsVersion();
        }
    }

    // Timestamp must be always in the past, but not too far,
    // so that some operators or vaults can be already unregistered.
    modifier validTimestamp(uint48 ts) {
        _validTimestamp(ts);
        _;
    }

    function _validTimestamp(uint48 ts) internal view {
        Storage storage $ = _storage();
        if (ts >= Time.timestamp()) {
            revert IncorrectTimestamp();
        }

        uint48 gracePeriod = $.operatorGracePeriod < $.vaultGracePeriod ? $.operatorGracePeriod : $.vaultGracePeriod;
        if (ts + gracePeriod <= Time.timestamp()) {
            revert IncorrectTimestamp();
        }
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

    modifier vaultOwner(address vault) {
        _vaultOwner(vault);
        _;
    }

    function _vaultOwner(address vault) internal view {
        if (!IAccessControl(vault).hasRole(DEFAULT_ADMIN_ROLE, msg.sender)) {
            revert NotVaultOwner();
        }
    }
}
