// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

interface IPOAMiddleware {
    struct POAStorage {
        address[] operators;
    }

    function setValidators(address[] memory validators) external;
}
