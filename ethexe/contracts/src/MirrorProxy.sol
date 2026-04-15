// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.33;

/**
 * @dev This is custom implementation of ERC-1167 (Minimal Proxy Contract)
 *      that supports upgrades: https://eips.ethereum.org/EIPS/eip-1167.
 *
 *      Unlike ERC1167, which stores `implementation` address in bytecode
 *      and then does `delegatecall` on it, we store `ROUTER` address
 *      in bytecode and call `IRouter(ROUTER.mirrorImpl())` to get
 *      last address of latest `Mirror` implementation.
 *
 *      Used for contracts that were written using Sails framework.
 *      Here we also define methods and public `Mirror` fields such as
 *      `sendMessage`, `sendReply`, etc. to be displayed on "Write Contract"
 *      tab on Etherscan. Sails smart contract methods will be displayed on
 *      "Write As Proxy" tab on Etherscan.
 *
 *      How it works:
 *      1. User calls `IRouter.createProgram(bytes32 codeId, bytes32 salt, address overrideInitializer)`
 *         and it returns address of new `Mirror` contract (e.g. `0x123...`).
 *         Each `Mirror` is about 670 bytes in size (in current `MirrorProxy` implementation).
 *
 *         Look at the implementation of `function _createProgram(bytes32 _codeId, bytes32 _salt, bool _isSmall)` in `Router`.
 *         As you can see, it uses the `Clones` / `ClonesSmall` library, which will ultimately lead to the creation of contract
 *         with the bytecode `MirrorProxy` / `MirrorProxySmall`.
 *      2. Once this small `Mirror` smart contract is created, it references most recent
 *         `Mirror` implementation
 *
 *      User/EOA (call)
 *        -> newly created `Mirror` (`0x123...`)
 *        -> `MirrorProxy.fallback()`
 *        -> `MirrorProxy._delegate()`
 *        -> (delegate call) to `IRouter(ROUTER).mirrorImpl()`
 *        -> `Mirror` implementation (e.g. `0xabc...`), see `Mirror.sol`
 *
 *      Owner of `Router` can call `IRouter.setMirror(address newMirror)` and instantly update
 *      implementations of all old created `Mirror`s.
 */
contract MirrorProxy {
    /**
     * @dev The address of the router contract.
     *      It will be automatically replaced with the correct address during deployment by
     *      `./ethexe/scripts/deploy-ethereum-contracts.sh` script.
     */
    address internal constant ROUTER = 0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE;

    /* # Public storage variables, useful for Etherscan */

    bytes32 public stateHash;
    uint256 public nonce;
    bool public exited;
    address public inheritor;
    address public initializer;

    constructor() payable {}

    /* # Primary Gear logic (external calls), see `IMirror`/`Mirror` for details */

    function sendMessage(bytes calldata payload, bool callReply) external payable /*returns (bytes32)*/  {
        _delegate();
    }

    function sendReply(bytes32 repliedTo, bytes calldata payload) external payable {
        _delegate();
    }

    function claimValue(bytes32 claimedId) external {
        _delegate();
    }

    function executableBalanceTopUp(uint128 value) external {
        _delegate();
    }

    function transferLockedValueToInheritor() external {
        _delegate();
    }

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
