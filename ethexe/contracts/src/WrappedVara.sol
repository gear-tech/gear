// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

import {OwnableUpgradeable} from "@openzeppelin/contracts-upgradeable/access/OwnableUpgradeable.sol";
import {Initializable} from "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import {ERC20Upgradeable} from "@openzeppelin/contracts-upgradeable/token/ERC20/ERC20Upgradeable.sol";
import {
    ERC20BurnableUpgradeable
} from "@openzeppelin/contracts-upgradeable/token/ERC20/extensions/ERC20BurnableUpgradeable.sol";
import {
    ERC20PermitUpgradeable
} from "@openzeppelin/contracts-upgradeable/token/ERC20/extensions/ERC20PermitUpgradeable.sol";
import {UUPSUpgradeable} from "@openzeppelin/contracts/proxy/utils/UUPSUpgradeable.sol";

/**
 * @dev Wrapped Vara (WVARA) is represents VARA on Ethereum as ERC20 token.
 *      VARA is also used for paying fees, staking and governance on Vara Network,
 *      while WVARA does all of the same things but on Ethereum.
 *
 *      On Ethereum network, WVARA is used as an executable balance for programs (Mirrors).
 *
 *      Please note that this version of WrappedVara is only used in local development environments,
 *      in production we use this:
 *      - https://github.com/gear-tech/gear-bridges/blob/main/ethereum/src/erc20/WrappedVara.sol
 */
contract WrappedVara is
    Initializable,
    ERC20Upgradeable,
    ERC20BurnableUpgradeable,
    OwnableUpgradeable,
    ERC20PermitUpgradeable,
    UUPSUpgradeable
{
    string private constant TOKEN_NAME = "Wrapped Vara";
    string private constant TOKEN_SYMBOL = "WVARA";
    uint256 private constant TOKEN_INITIAL_SUPPLY = 1_000_000;

    /**
     * @custom:oz-upgrades-unsafe-allow constructor
     */
    constructor() {
        _disableInitializers();
    }

    /**
     * @dev Initializes the WrappedVara contract with the token name and symbol.
     * @param initialOwner The address that will be able to mint tokens.
     * @dev The initialOwner receives the 1 million WVARA tokens minted during initialization.
     */
    function initialize(address initialOwner) public initializer {
        __ERC20_init(TOKEN_NAME, TOKEN_SYMBOL);
        __ERC20Burnable_init();
        __Ownable_init(initialOwner);
        __ERC20Permit_init(TOKEN_NAME);

        _mint(initialOwner, TOKEN_INITIAL_SUPPLY * 10 ** decimals());
    }

    /**
     * @custom:oz-upgrades-validate-as-initializer
     */
    function reinitialize() public onlyOwner reinitializer(2) {
        __ERC20_init(TOKEN_NAME, TOKEN_SYMBOL);
        __ERC20Burnable_init();
        __Ownable_init(owner());
        __ERC20Permit_init(TOKEN_NAME);
    }

    /**
     * @dev Function that should revert when `msg.sender` is not authorized to upgrade the contract.
     *      Called by {upgradeToAndCall}.
     */
    function _authorizeUpgrade(address newImplementation) internal override onlyOwner {}

    /**
     * @dev Returns the number of decimals used to get its user representation.
     *      Also see documentation about decimals:
     *      - https://wiki.vara.network/docs/vara-network/staking/validator-faqs#what-is-the-precision-of-the-vara-token
     */
    function decimals() public pure override returns (uint8) {
        return 12;
    }

    /**
     * @dev Mints `amount` tokens to `to`.
     * @param to The address to mint tokens to.
     * @param amount The amount of tokens to mint.
     */
    function mint(address to, uint256 amount) public onlyOwner {
        _mint(to, amount);
    }
}
