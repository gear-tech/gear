// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";

library MapWithTimeData {
    using EnumerableMap for EnumerableMap.AddressToUintMap;

    /**
     * @dev Thrown when an address is already added to the map.
     */
    error AlreadyAdded();

    /**
     * @dev Thrown when an address is not enabled.
     */
    error NotEnabled();

    /**
     * @dev Thrown when an address is already enabled.
     */
    error AlreadyEnabled();

    /**
     * @dev Converts uint256 value to its component parts.
     * @param value The uint256 value to convert.
     * @return enabledTime The time when the entry was enabled.
     * @return disabledTime The time when the entry was disabled.
     * @return data The associated data.
     */
    function toInner(uint256 value) private pure returns (uint48 enabledTime, uint48 disabledTime, uint160 data) {
        // casting to 'uint48' is safe because this is toInner method
        // forge-lint: disable-next-line(unsafe-typecast)
        return (uint48(value), uint48(value >> 48), uint160(value >> 96));
    }

    /**
     * @dev Converts component parts to single uint256 value.
     * @param enabledTime The time when entry was enabled.
     * @param disabledTime The time when entry was disabled.
     * @param data The associated data.
     * @return The combined uint256 value.
     */
    function toValue(uint48 enabledTime, uint48 disabledTime, uint160 data) private pure returns (uint256) {
        return uint256(enabledTime) | (uint256(disabledTime) << 48) | (uint256(data) << 96);
    }

    /**
     * @dev Appends new entry to map with current timestamp as enabled time.
     * @param self The map to append to.
     * @param addr The address key for new entry.
     * @param data The associated data for new entry.
     * @notice Reverts if address is already added to map.
     */
    function append(EnumerableMap.AddressToUintMap storage self, address addr, uint160 data) internal {
        if (!self.set(addr, toValue(Time.timestamp(), 0, data))) {
            revert AlreadyAdded();
        }
    }

    function enable(EnumerableMap.AddressToUintMap storage self, address addr) internal {
        (uint48 enabledTime, uint48 disabledTime, uint160 data) = toInner(self.get(addr));

        if (enabledTime != 0 && disabledTime == 0) {
            revert AlreadyEnabled();
        }

        self.set(addr, toValue(Time.timestamp(), 0, data));
    }

    function disable(EnumerableMap.AddressToUintMap storage self, address addr) internal {
        (uint48 enabledTime, uint48 disabledTime, uint160 data) = toInner(self.get(addr));

        if (enabledTime == 0 || disabledTime != 0) {
            revert NotEnabled();
        }

        self.set(addr, toValue(enabledTime, Time.timestamp(), data));
    }

    function atWithTimes(EnumerableMap.AddressToUintMap storage self, uint256 idx)
        internal
        view
        returns (address key, uint48 enabledTime, uint48 disabledTime)
    {
        uint256 value;
        (key, value) = self.at(idx);
        (enabledTime, disabledTime,) = toInner(value);
    }

    function getTimes(EnumerableMap.AddressToUintMap storage self, address addr)
        internal
        view
        returns (uint48 enabledTime, uint48 disabledTime)
    {
        (enabledTime, disabledTime,) = toInner(self.get(addr));
    }

    function getPinnedData(EnumerableMap.AddressToUintMap storage self, address addr)
        internal
        view
        returns (uint160 data)
    {
        (,, data) = toInner(self.get(addr));
    }
}
