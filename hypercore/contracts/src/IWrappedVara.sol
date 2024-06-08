// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

interface IWrappedVara {
    function gasToValue(uint64 gas) external view returns (uint256);

    function transferFrom(address from, address to, uint256 value) external returns (bool);
}
