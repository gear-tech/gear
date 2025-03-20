// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

interface IPing {
    function ping(uint128 value, uint32 param1, string calldata param2) external;
}

contract Ping is IPing {
    function ping(uint128 value, uint32 param1, string calldata param2) external {}
}
