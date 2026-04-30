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
        uint256 size = 0x02fb;
        uint256 memPtr = Memory.allocate(size);

        /**
         * @dev This bytecode is taken from: `cat out/MirrorProxy.sol/MirrorProxy.json | jq -r ".bytecode.object"`
         */
        Memory.writeWord(memPtr, 0x0000, 0x6080806040526102e990816100128239f3fe60806040526004361061029f575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806336a52a18146100bb57806342129d00146100b65780635ce6c327);
        Memory.writeWord(memPtr, 0x0040, 0x146100b1578063701da98e146100ac578063704ed542146100a75780637a8e0c);
        Memory.writeWord(memPtr, 0x0060, 0xdd146100a257806391d5a64c1461009d5780639ce110d714610098578063affe);
        Memory.writeWord(memPtr, 0x0080, 0xd0e014610093578063c60496921461008e5763e43f34330361029f5761028a56);
        Memory.writeWord(memPtr, 0x00a0, 0x5b610260565b610243565b61021b565b610205565b6101d2565b6101b3565b61);
        Memory.writeWord(memPtr, 0x00c0, 0x0178565b610156565b610119565b346100e7575f3660031901126100e7576002);
        Memory.writeWord(memPtr, 0x00e0, 0x5460405160089190911c6001600160a01b03168152602090f35b5f80fd5b9181);
        Memory.writeWord(memPtr, 0x0100, 0x601f840112156100e75782359167ffffffffffffffff83116100e75760208381);
        Memory.writeWord(memPtr, 0x0120, 0x8601950101116100e757565b60403660031901126100e75760043567ffffffff);
        Memory.writeWord(memPtr, 0x0140, 0xffffffff81116100e7576101459036906004016100eb565b5050602435801515);
        Memory.writeWord(memPtr, 0x0160, 0x1461029f575f80fd5b346100e7575f3660031901126100e757602060ff600254);
        Memory.writeWord(memPtr, 0x0180, 0x166040519015158152f35b346100e7575f3660031901126100e75760205f5460);
        Memory.writeWord(memPtr, 0x01a0, 0x4051908152f35b600435906fffffffffffffffffffffffffffffffff82168203);
        Memory.writeWord(memPtr, 0x01c0, 0x6100e757565b346100e75760203660031901126100e7576101cc610194565b50);
        Memory.writeWord(memPtr, 0x01e0, 0x61029f565b60403660031901126100e75760243567ffffffffffffffff811161);
        Memory.writeWord(memPtr, 0x0200, 0x00e7576101fe9036906004016100eb565b505061029f565b346100e757602036);
        Memory.writeWord(memPtr, 0x0220, 0x60031901121561029f575f80fd5b346100e7575f3660031901126100e7576003);
        Memory.writeWord(memPtr, 0x0240, 0x546040516001600160a01b039091168152602090f35b346100e7575f36600319);
        Memory.writeWord(memPtr, 0x0260, 0x01126100e7576020600154604051908152f35b346100e75760a0366003190112);
        Memory.writeWord(memPtr, 0x0280, 0x6100e757610279610194565b5060443560ff81161461029f575f80fd5b346100);
        Memory.writeWord(memPtr, 0x02a0, 0xe7575f3660031901121561029f575f80fd5b63e6fabc0960e01b5f5260205f60);
        /**
         * @dev Write `Router` address into the deployed bytecode.
         */
        Memory.writeWord(
            memPtr,
            0x02c0,
            (0x04817300000000000000000000000000000000000000005afa156100e7575f80) | (uint256(uint160(router)) << 72)
        );
        Memory.writeWord(memPtr, 0x02e0, 0x8051368280378136915af43d5f803e156102e5573d5ff35b3d5ffd0000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
