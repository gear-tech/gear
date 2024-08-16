// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

interface IMirrorProxy {
    function router() external view returns (address);
}
