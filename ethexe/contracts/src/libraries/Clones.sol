// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;

import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

library Clones {
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        uint256 size = 0x02c4;
        uint256 memPtr = Memory.allocate(size);
        Memory.writeWord(memPtr, 0x0000, 0x6080806040526102b290816100128239f3fe608060405260043610610268575f);
        Memory.writeWord(memPtr, 0x0020, 0x3560e01c806329336f391461009b5780632a68e35d1461009657806336a52a18);
        Memory.writeWord(memPtr, 0x0040, 0x14610091578063701da98e1461008c578063704ed5421461008757806391d5a6);
        Memory.writeWord(memPtr, 0x0060, 0x4c146100825780639ce110d71461007d578063affed0e0146100785763e43f34);
        Memory.writeWord(memPtr, 0x0080, 0x330361026857610253565b610236565b61020e565b6101f8565b6101df565b61);
        Memory.writeWord(memPtr, 0x00a0, 0x01c2565b61019b565b610150565b346100dc5760603660031901126100dc5760);
        Memory.writeWord(memPtr, 0x00c0, 0x243567ffffffffffffffff81116100dc576100cc9036906004016100e0565b50);
        Memory.writeWord(memPtr, 0x00e0, 0x506100d661010e565b50610268565b5f80fd5b9181601f840112156100dc5782);
        Memory.writeWord(memPtr, 0x0100, 0x359167ffffffffffffffff83116100dc57602083818601950101116100dc5756);
        Memory.writeWord(memPtr, 0x0120, 0x5b604435906001600160801b03821682036100dc57565b602435906001600160);
        Memory.writeWord(memPtr, 0x0140, 0x801b03821682036100dc57565b600435906001600160801b03821682036100dc);
        Memory.writeWord(memPtr, 0x0160, 0x57565b346100dc5760603660031901126100dc5760043567ffffffffffffffff);
        Memory.writeWord(memPtr, 0x0180, 0x81116100dc576101819036906004016100e0565b505061018b610124565b5060);
        Memory.writeWord(memPtr, 0x01a0, 0x443580151514610268575f80fd5b346100dc575f3660031901126100dc575f54);
        Memory.writeWord(memPtr, 0x01c0, 0x6040516001600160a01b039091168152602090f35b346100dc575f3660031901);
        Memory.writeWord(memPtr, 0x01e0, 0x126100dc576020600254604051908152f35b346100dc57602036600319011261);
        Memory.writeWord(memPtr, 0x0200, 0x00dc576100d661013a565b346100dc57602036600319011215610268575f80fd);
        Memory.writeWord(memPtr, 0x0220, 0x5b346100dc575f3660031901126100dc576001546040516001600160a01b0390);
        Memory.writeWord(memPtr, 0x0240, 0x91168152602090f35b346100dc575f3660031901126100dc5760206003546040);
        Memory.writeWord(memPtr, 0x0260, 0x51908152f35b346100dc575f36600319011215610268575f80fd5b63e6fabc09);
        Memory.writeWord(
            memPtr,
            0x0280,
            (0x60e01b5f5260205f600481730000000000000000000000000000000000000000) | (uint256(uint160(router)))
        );
        Memory.writeWord(memPtr, 0x02a0, 0x5afa156100dc575f808051368280378136915af43d5f803e156102ae573d5ff3);
        Memory.writeWord(memPtr, 0x02c0, 0x5b3d5ffd00000000000000000000000000000000000000000000000000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
