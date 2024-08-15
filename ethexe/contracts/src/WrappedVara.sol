// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {ERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import {ERC20BurnableUpgradeable} from
    "@openzeppelin/contracts-upgradeable/token/ERC20/extensions/ERC20BurnableUpgradeable.sol";
import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {ERC20PermitUpgradeable} from
    "@openzeppelin/contracts-upgradeable/token/ERC20/extensions/ERC20PermitUpgradeable.sol";

contract WrappedVara is
    Initializable,
    ERC20Upgradeable,
    ERC20BurnableUpgradeable,
    OwnableUpgradeable,
    ERC20PermitUpgradeable
{
    string private constant TOKEN_NAME = "Wrapped Vara";
    string private constant TOKEN_SYMBOL = "WVARA";
    uint256 private constant TOKEN_INITIAL_SUPPLY = 1_000_000;

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address initialOwner) public initializer {
        __ERC20_init(TOKEN_NAME, TOKEN_SYMBOL);
        __ERC20Burnable_init();
        __Ownable_init(initialOwner);
        __ERC20Permit_init(TOKEN_NAME);

        _mint(initialOwner, TOKEN_INITIAL_SUPPLY * 10 ** decimals());
    }

    function reinitialize() public onlyOwner reinitializer(2) {}

    function mint(address to, uint256 amount) public onlyOwner {
        _mint(to, amount);
    }
}
