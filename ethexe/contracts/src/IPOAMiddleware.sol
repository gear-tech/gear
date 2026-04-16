// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

/**
 * @dev Interface for the POAMiddleware contract.
 */
interface IPOAMiddleware {
    // actually MiddlewareV1, but we need to pass OpenZeppelin's upgradeability checks
    /**
     * @custom:storage-location erc7201:middleware.storage.MiddlewareV2
     */
    struct POAStorage {
        address[] operators;
    }

    /**
     * @dev Sets validators for POA middleware.
     * @param validators The addresses of validators to set.
     */
    function setValidators(address[] memory validators) external;
}
