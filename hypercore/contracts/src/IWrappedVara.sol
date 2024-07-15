// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {IERC20Metadata} from "@openzeppelin/contracts/token/ERC20/extensions/IERC20Metadata.sol";

interface IWrappedVara is IERC20, IERC20Metadata {
    function valuePerGas() external view returns (uint128);

    function setValuePerGas(uint128 _valuePerGas) external;

    function gasToValue(uint64 gas) external view returns (uint256);
}
