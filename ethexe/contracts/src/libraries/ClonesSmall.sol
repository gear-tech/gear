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
 *      `MirrorProxySmall` smart contract, see its code for details.
 * @dev https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Clones.sol
 */
library ClonesSmall {
    /**
     * @dev Deploys and returns the address of clone that has the `MirrorProxySmall` behavior.
     */
    function cloneDeterministic(address router, bytes32 salt) internal returns (address instance) {
        return cloneDeterministic(router, salt, 0);
    }

    /**
     * @dev Same as `cloneDeterministic(address router, bytes32 salt)`, but with
     * `value` parameter to send native currency to the new contract.
     */
    function cloneDeterministic(address router, bytes32 salt, uint256 value) internal returns (address instance) {
        uint256 size = 0x5a;
        uint256 memPtr = Memory.allocate(size);

        /**
         * @dev This bytecode is taken from `cat out/MirrorProxySmall.sol/MirrorProxySmall.json | jq -r ".deployedBytecode.object"`
         *      The bytecode "0x3d605080600a3d3981f3" is responsible for deploy and is modified version of ERC1167 from OpenZeppelin:
         *      - https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Clones.sol
         */
        Memory.writeWord(memPtr, 0x00, 0x3d605080600a3d3981f3608060405263e6fabc0960e01b5f5260205f60048173);
        Memory.writeWord(
            memPtr,
            0x20,
            (0x00000000000000000000000000000000000000005afa15604c575f8080513682) | (uint256(uint160(router)) << 0x60)
        );
        Memory.writeWord(memPtr, 0x40, 0x80378136915af43d5f803e156048573d5ff35b3d5ffd5b5f80fd000000000000);

        assembly ("memory-safe") {
            instance := create2(value, memPtr, size, salt)
            if iszero(instance) { revert(0x00, 0x00) }
        }
    }
}
