// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

library Clones {
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        uint256 size = 0x02b4;
        uint256 memPtr = Memory.allocate(size);

        Memory.writeWord(memPtr, 0x0000, 0x6080806040526102a290816100128239f3fe608060405260043610610258575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806329336f391461009b57806336a52a1814610096578063701da98e);
        Memory.writeWord(memPtr, 0x0040, 0x14610091578063704ed5421461008c57806391d5a64c146100875780639ce110);
        Memory.writeWord(memPtr, 0x0060, 0xd714610082578063affed0e01461007d578063d5624222146100785763e43f34);
        Memory.writeWord(memPtr, 0x0080, 0x330361025857610243565b610208565b6101eb565b6101c3565b6101ad565b61);
        Memory.writeWord(memPtr, 0x00a0, 0x0194565b610177565b610150565b346100dc5760603660031901126100dc5760);
        Memory.writeWord(memPtr, 0x00c0, 0x243567ffffffffffffffff81116100dc576100cc9036906004016100e0565b50);
        Memory.writeWord(memPtr, 0x00e0, 0x506100d661010e565b50610258565b5f80fd5b9181601f840112156100dc5782);
        Memory.writeWord(memPtr, 0x0100, 0x359167ffffffffffffffff83116100dc57602083818601950101116100dc5756);
        Memory.writeWord(memPtr, 0x0120, 0x5b604435906001600160801b03821682036100dc57565b600435906001600160);
        Memory.writeWord(memPtr, 0x0140, 0x801b03821682036100dc57565b602435906001600160801b03821682036100dc);
        Memory.writeWord(memPtr, 0x0160, 0x57565b346100dc575f3660031901126100dc575f546040516001600160a01b03);
        Memory.writeWord(memPtr, 0x0180, 0x9091168152602090f35b346100dc575f3660031901126100dc57602060025460);
        Memory.writeWord(memPtr, 0x01a0, 0x4051908152f35b346100dc5760203660031901126100dc576100d6610124565b);
        Memory.writeWord(memPtr, 0x01c0, 0x346100dc57602036600319011215610258575f80fd5b346100dc575f36600319);
        Memory.writeWord(memPtr, 0x01e0, 0x01126100dc576001546040516001600160a01b039091168152602090f35b3461);
        Memory.writeWord(memPtr, 0x0200, 0x00dc575f3660031901126100dc576020600354604051908152f35b346100dc57);
        Memory.writeWord(memPtr, 0x0220, 0x60403660031901126100dc5760043567ffffffffffffffff81116100dc576102);
        Memory.writeWord(memPtr, 0x0240, 0x399036906004016100e0565b50506100d661013a565b346100dc575f36600319);
        Memory.writeWord(
            memPtr,
            0x0260,
            (0x011215610258575f80fd5b63e6fabc0960e01b5f5260205f6004817300000000)
                | ((uint256(uint160(router)) >> 0x80) & 0xFFFFFFFF)
        );
        Memory.writeWord(
            memPtr,
            0x0280,
            (0x000000000000000000000000000000005afa156100dc575f8080513682803781) | (uint256(uint160(router)) << 0x80)
        );
        Memory.writeWord(memPtr, 0x02a0, 0x36915af43d5f803e1561029e573d5ff35b3d5ffd000000000000000000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
