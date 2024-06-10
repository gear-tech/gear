// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ERC20Burnable} from "@openzeppelin/contracts/token/ERC20/extensions/ERC20Burnable.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract WrappedVara is ERC20, ERC20Burnable, Ownable {
    uint256 public constant INITIAL_SUPPLY = 1_000_000;
    uint128 public valuePerGas;

    constructor(address initialOwner, uint128 _valuePerGas) ERC20("Wrapped Vara", "WVARA") Ownable(initialOwner) {
        _mint(initialOwner, INITIAL_SUPPLY * 10 ** decimals());
        setValuePerGas(_valuePerGas);
    }

    function setValuePerGas(uint128 _valuePerGas) public onlyOwner {
        require(_valuePerGas > 0, "valuePerGas must be greater than zero");
        valuePerGas = _valuePerGas;
    }

    function gasToValue(uint64 gas) public view returns (uint256) {
        return gas * valuePerGas;
    }

    function mint(address to, uint256 amount) public onlyOwner {
        _mint(to, amount);
    }

    function decimals() public view virtual override returns (uint8) {
        return 12;
    }
}
