// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";

library MapWithTimeData {
    using EnumerableMap for EnumerableMap.AddressToUintMap;

    error AlreadyAdded();
    error NotEnabled();
    error AlreadyEnabled();

    function toInner(uint256 value) private pure returns (uint48 enabledTime, uint48 disabledTime, uint160 data) {
        // casting to 'uint48' is safe because this is toInner method
        // forge-lint: disable-next-line(unsafe-typecast)
        return (uint48(value), uint48(value >> 48), uint160(value >> 96));
    }

    function toValue(uint48 enabledTime, uint48 disabledTime, uint160 data) private pure returns (uint256) {
        return uint256(enabledTime) | (uint256(disabledTime) << 48) | (uint256(data) << 96);
    }

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
