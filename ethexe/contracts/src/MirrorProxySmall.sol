// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

/// @dev This is custom implementation of ERC1167 that supports upgrades.
///
///      Unlike ERC1167, which stores `implementation` address in bytecode
///      and then does `delegatecall` on it, we store `ROUTER` address
///      in bytecode and call `IRouter(ROUTER.mirrorImpl())` to get
///      last address of latest `Mirror` implementation.
///
///      Used for contracts that have their own communication protocol
///      (contracts not using Sails framework).
contract MirrorProxySmall {
    address internal constant ROUTER = 0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE;

    constructor() payable {}

    /* MirrorProxy implementation */

    function _delegate() internal {
        assembly {
            // IRouter.mirrorImpl.selector = bytes4(0xe6fabc09)
            mstore(0, shl(224, 0xe6fabc09))
            let success := staticcall(gas(), ROUTER, 0, 4, 0, 32)
            if iszero(success) { revert(0, 0) }
            let implementation := mload(0)

            // https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Proxy.sol
            calldatacopy(0, 0, calldatasize())
            let result := delegatecall(gas(), implementation, 0, calldatasize(), 0, 0)
            returndatacopy(0, 0, returndatasize())
            switch result
            case 0 { revert(0, returndatasize()) }
            default { return(0, returndatasize()) }
        }
    }

    fallback() external payable {
        _delegate();
    }
}
