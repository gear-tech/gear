// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.33;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

/**
 * @dev ERC-1167 (Minimal Proxy Contract) is a standard for
 *      deploying minimal proxy contracts, also known as "clones":
 *      https://eips.ethereum.org/EIPS/eip-1167.
 *
 *      > To simply and cheaply clone contract functionality in an immutable way, this standard specifies
 *      > a minimal bytecode implementation that delegates all calls to a known, fixed address.
 *
 *      The library includes functions to deploy a proxy using `create2` (salted deterministic deployment).
 *
 *      However, it's worth noting that this is custom ERC-1167 implementation. All this library does is deploy
 *      `MirrorProxy` smart contract, see its code for details.
 * @dev https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Clones.sol
 */
library Clones {
    /**
     * @dev Deploys and returns the address of clone that has the `MirrorProxy` behavior.
     */
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    /**
     * @dev Same as `cloneDeterministic(address router, bytes32 salt)`, but with
     * `value` parameter to send native currency to the new contract.
     */
    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        /**
         * @dev Size is taken from second column: `forge build --sizes | grep "| MirrorProxy "`.
         */
        uint256 size = 0x03fd;
        uint256 memPtr = Memory.allocate(size);

        /**
         * @dev This bytecode is taken from: `cat out/MirrorProxy.sol/MirrorProxy.json | jq -r ".bytecode.object"`
         */
        Memory.writeWord(memPtr, 0x0000, 0x6080806040526103eb90816100128239f3fe6080604052600436106103a1575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806301b4832c146100eb57806336a52a18146100e657806342129d00);
        Memory.writeWord(memPtr, 0x0040, 0x146100e15780635ce6c327146100dc578063701da98e146100d7578063704ed5);
        Memory.writeWord(memPtr, 0x0060, 0x42146100d25780637a8e0cdd146100cd57806391d5a64c146100c85780639ce1);
        Memory.writeWord(memPtr, 0x0080, 0x10d7146100c3578063a984a575146100be578063affed0e0146100b9578063c6);
        Memory.writeWord(memPtr, 0x00a0, 0x049692146100b4578063e43f3433146100af5763ed3513fc036103a157610328);
        Memory.writeWord(memPtr, 0x00c0, 0x565b610313565b6102e9565b6102cc565b61029d565b610275565b61025f565b);
        Memory.writeWord(memPtr, 0x00e0, 0x61022c565b61020d565b6101d2565b6101b0565b610173565b610119565b3461);
        Memory.writeWord(memPtr, 0x0100, 0x0115576020366003190112610115576004355f526004602052602060405f2054);
        Memory.writeWord(memPtr, 0x0120, 0x604051908152f35b5f80fd5b34610115575f3660031901126101155760025460);
        Memory.writeWord(memPtr, 0x0140, 0x405160089190911c6001600160a01b03168152602090f35b9181601f84011215);
        Memory.writeWord(memPtr, 0x0160, 0x6101155782359167ffffffffffffffff83116101155760208381860195010111);
        Memory.writeWord(memPtr, 0x0180, 0x61011557565b60403660031901126101155760043567ffffffffffffffff8111);
        Memory.writeWord(memPtr, 0x01a0, 0x6101155761019f903690600401610145565b5050602435801515146103a1575f);
        Memory.writeWord(memPtr, 0x01c0, 0x80fd5b34610115575f36600319011261011557602060ff600254166040519015);
        Memory.writeWord(memPtr, 0x01e0, 0x158152f35b34610115575f3660031901126101155760205f54604051908152f3);
        Memory.writeWord(memPtr, 0x0200, 0x5b600435906fffffffffffffffffffffffffffffffff8216820361011557565b);
        Memory.writeWord(memPtr, 0x0220, 0x34610115576020366003190112610115576102266101ee565b506103a1565b60);
        Memory.writeWord(memPtr, 0x0240, 0x403660031901126101155760243567ffffffffffffffff811161011557610258);
        Memory.writeWord(memPtr, 0x0260, 0x903690600401610145565b50506103a1565b3461011557602036600319011215);
        Memory.writeWord(memPtr, 0x0280, 0x6103a1575f80fd5b34610115575f366003190112610115576003546040516001);
        Memory.writeWord(memPtr, 0x02a0, 0x600160a01b039091168152602090f35b34610115576020366003190112610115);
        Memory.writeWord(memPtr, 0x02c0, 0x576004355f526005602052602060ff60405f2054166040519015158152f35b34);
        Memory.writeWord(memPtr, 0x02e0, 0x610115575f366003190112610115576020600154604051908152f35b34610115);
        Memory.writeWord(memPtr, 0x0300, 0x5760a0366003190112610115576103026101ee565b5060443560ff8116146103);
        Memory.writeWord(memPtr, 0x0320, 0xa1575f80fd5b34610115575f366003190112156103a1575f80fd5b3461011557);
        Memory.writeWord(memPtr, 0x0340, 0x60a03660031901126101155760643567ffffffffffffffff8111610115576103);
        Memory.writeWord(memPtr, 0x0360, 0x59903690600401610145565b505060843567ffffffffffffffff811161011557);
        Memory.writeWord(memPtr, 0x0380, 0x366023820112156101155780600401359067ffffffffffffffff821161011557);
        Memory.writeWord(memPtr, 0x03a0, 0x602490369260051b010111156103a1575f80fd5b63e6fabc0960e01b5f526020);
        /**
         * @dev Write `Router` address into the deployed bytecode.
         */
        Memory.writeWord(
            memPtr,
            0x03c0,
            (0x5f6004817300000000000000000000000000000000000000005afa1561011557) | (uint256(uint160(router)) << 56)
        );
        Memory.writeWord(memPtr, 0x03e0, 0x5f808051368280378136915af43d5f803e156103e7573d5ff35b3d5ffd000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
