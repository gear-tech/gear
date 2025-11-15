// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

library Clones {
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        uint256 size = 0x02b0;
        uint256 memPtr = Memory.allocate(size);

        /// @dev This bytecode is taken from `cat out/MirrorProxy.sol/MirrorProxy.json | jq -r ".bytecode.object"`
        Memory.writeWord(memPtr, 0x0000, 0x60808060405261029e90816100128239f3fe608060405260043610610254575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806336a52a18146100ab57806342129d00146100a65780635ce6c327);
        Memory.writeWord(memPtr, 0x0040, 0x146100a1578063701da98e1461009c578063704ed542146100975780637a8e0c);
        Memory.writeWord(memPtr, 0x0060, 0xdd1461009257806391d5a64c1461008d5780639ce110d714610088578063affe);
        Memory.writeWord(memPtr, 0x0080, 0xd0e0146100835763e43f3433036102545761023f565b610222565b6101fa565b);
        Memory.writeWord(memPtr, 0x00a0, 0x6101e4565b6101b1565b610181565b610165565b610143565b610106565b3461);
        Memory.writeWord(memPtr, 0x00c0, 0x00d4575f3660031901126100d45760025460081c6001600160a01b0316608090);
        Memory.writeWord(memPtr, 0x00e0, 0x8152602090f35b5f80fd5b9181601f840112156100d45782359167ffffffffff);
        Memory.writeWord(memPtr, 0x0100, 0xffffff83116100d457602083818601950101116100d457565b60403660031901);
        Memory.writeWord(memPtr, 0x0120, 0x126100d45760043567ffffffffffffffff81116100d457610132903690600401);
        Memory.writeWord(memPtr, 0x0140, 0x6100d8565b505060243580151514610254575f80fd5b346100d4575f36600319);
        Memory.writeWord(memPtr, 0x0160, 0x01126100d457602060ff600254166040519015158152f35b346100d4575f3660);
        Memory.writeWord(memPtr, 0x0180, 0x031901126100d45760205f54604051908152f35b346100d45760203660031901);
        Memory.writeWord(memPtr, 0x01a0, 0x126100d4576004356fffffffffffffffffffffffffffffffff81161461025457);
        Memory.writeWord(memPtr, 0x01c0, 0x5f80fd5b60403660031901126100d45760243567ffffffffffffffff81116100);
        Memory.writeWord(memPtr, 0x01e0, 0xd4576101dd9036906004016100d8565b5050610254565b346100d45760203660);
        Memory.writeWord(memPtr, 0x0200, 0x0319011215610254575f80fd5b346100d4575f3660031901126100d457600354);
        Memory.writeWord(memPtr, 0x0220, 0x6040516001600160a01b039091168152602090f35b346100d4575f3660031901);
        Memory.writeWord(memPtr, 0x0240, 0x126100d4576020600154604051908152f35b346100d4575f3660031901121561);
        Memory.writeWord(
            memPtr,
            0x0260,
            (0x0254575f80fd5b63e6fabc0960e01b5f5260205f600481730000000000000000) | ((uint256(uint160(router)) >> 0x60))
        );
        Memory.writeWord(
            memPtr,
            0x0280,
            (((uint256(uint160(router)) << 0xa0)
                        | (0x0000000000000000000000005afa156100d4575f808051368280378136915af4)))
        );
        Memory.writeWord(memPtr, 0x02a0, 0x3d5f803e1561029a573d5ff35b3d5ffd00000000000000000000000000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
