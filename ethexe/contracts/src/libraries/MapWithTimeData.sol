// SPDX-License-Identifier: MIT
pragma solidity ^0.8.25;

import {Checkpoints} from "@openzeppelin/contracts/utils/structs/Checkpoints.sol";
import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";
import {EnumerableMap} from "@openzeppelin/contracts/utils/structs/EnumerableMap.sol";

library MapWithTimeData {
    using EnumerableMap for EnumerableMap.AddressToUintMap;

    error AlreadyAdded();
    error NotEnabled();
    error AlreadyEnabled();

    function toInner(uint256 value) private pure returns (uint48, uint48, address) {
        return (uint48(value), uint48(value >> 48), address(uint160(value >> 96)));
    }

    function toValue(uint48 enabledTime, uint48 disabledTime, address pinnedAddress) private pure returns (uint256) {
        return uint256(enabledTime) | (uint256(disabledTime) << 48) | (uint256(uint160(pinnedAddress)) << 96);
    }

    function append(EnumerableMap.AddressToUintMap storage self, address addr, address pinnedAddress) internal {
        if (!self.set(addr, toValue(Time.timestamp(), 0, pinnedAddress))) {
            revert AlreadyAdded();
        }
    }

    function enable(EnumerableMap.AddressToUintMap storage self, address addr) internal {
        (uint48 enabledTime, uint48 disabledTime, address pinnedAddress) = toInner(self.get(addr));

        if (enabledTime != 0 && disabledTime == 0) {
            revert AlreadyEnabled();
        }

        self.set(addr, toValue(Time.timestamp(), 0, pinnedAddress));
    }

    function disable(EnumerableMap.AddressToUintMap storage self, address addr) internal {
        (uint48 enabledTime, uint48 disabledTime, address pinnedAddress) = toInner(self.get(addr));

        if (enabledTime == 0 || disabledTime != 0) {
            revert NotEnabled();
        }

        self.set(addr, toValue(enabledTime, Time.timestamp(), pinnedAddress));
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

    function getPinnedAddress(EnumerableMap.AddressToUintMap storage self, address addr)
        internal
        view
        returns (address pinnedAddress)
    {
        (,, pinnedAddress) = toInner(self.get(addr));
    }
}
