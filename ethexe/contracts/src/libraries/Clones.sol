// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

library Clones {
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        uint256 size = 0x028b;
        uint256 memPtr = Memory.allocate(size);

        /// @dev This bytecode is taken from `cat out/MirrorProxy.sol/MirrorProxy.json | jq -r ".bytecode.object"`
        Memory.writeWord(memPtr, 0x0000, 0x60808060405261027990816100128239f3fe60806040526004361061022f575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806336a52a181461009b57806342129d00146100965780635ce6c327);
        Memory.writeWord(memPtr, 0x0040, 0x14610091578063701da98e1461008c578063704ed542146100875780637a8e0c);
        Memory.writeWord(memPtr, 0x0060, 0xdd1461008257806391d5a64c1461007d5780639ce110d7146100785763affed0);
        Memory.writeWord(memPtr, 0x0080, 0xe00361022f57610212565b6101ea565b6101d4565b6101a1565b610171565b61);
        Memory.writeWord(memPtr, 0x00a0, 0x0155565b610133565b6100f6565b346100c4575f3660031901126100c4576002);
        Memory.writeWord(memPtr, 0x00c0, 0x5460081c6001600160a01b03166080908152602090f35b5f80fd5b9181601f84);
        Memory.writeWord(memPtr, 0x00e0, 0x0112156100c45782359167ffffffffffffffff83116100c45760208381860195);
        Memory.writeWord(memPtr, 0x0100, 0x0101116100c457565b60403660031901126100c45760043567ffffffffffffff);
        Memory.writeWord(memPtr, 0x0120, 0xff81116100c4576101229036906004016100c8565b5050602435801515146102);
        Memory.writeWord(memPtr, 0x0140, 0x2f575f80fd5b346100c4575f3660031901126100c457602060ff600254166040);
        Memory.writeWord(memPtr, 0x0160, 0x519015158152f35b346100c4575f3660031901126100c45760205f5460405190);
        Memory.writeWord(memPtr, 0x0180, 0x8152f35b346100c45760203660031901126100c4576004356fffffffffffffff);
        Memory.writeWord(memPtr, 0x01a0, 0xffffffffffffffffff81161461022f575f80fd5b60403660031901126100c457);
        Memory.writeWord(memPtr, 0x01c0, 0x60243567ffffffffffffffff81116100c4576101cd9036906004016100c8565b);
        Memory.writeWord(memPtr, 0x01e0, 0x505061022f565b346100c45760203660031901121561022f575f80fd5b346100);
        Memory.writeWord(memPtr, 0x0200, 0xc4575f3660031901126100c4576003546040516001600160a01b039091168152);
        Memory.writeWord(memPtr, 0x0220, 0x602090f35b346100c4575f3660031901126100c4576020600154604051908152);
        Memory.writeWord(
            memPtr,
            0x0240,
            (0xf35b63e6fabc0960e01b5f5260205f6004817300000000000000000000000000) | ((uint256(uint160(router)) >> 0x38))
        );
        Memory.writeWord(
            memPtr,
            0x0260,
            (((uint256(uint160(router)) << 0xc8)
                        | (0x000000000000005afa156100c4575f808051368280378136915af43d5f803e15)))
        );
        Memory.writeWord(memPtr, 0x0280, 0x610275573d5ff35b3d5ffd000000000000000000000000000000000000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
