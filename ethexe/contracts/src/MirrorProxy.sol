// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

contract MirrorProxy {
    address internal constant ROUTER = 0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE;

    address public inheritor;
    address public initializer;
    bytes32 public stateHash;
    uint256 public nonce;

    constructor() payable {}

    /* Primary Gear logic */

    function sendMessage(bytes calldata payload, uint128 value) external /*returns (bytes32)*/ {
        _delegate();
    }

    function sendReply(bytes32 repliedTo, bytes calldata payload, uint128 value) external {
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
