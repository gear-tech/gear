// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

/// @title Gear.exe POAMiddleware Interface
interface IPOAMiddleware {
    struct PoaStorage {
        address[] operators;
    }

    function setValidators(address[] memory validators) external;
}
