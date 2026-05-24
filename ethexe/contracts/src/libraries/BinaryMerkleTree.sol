// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.33;

import {Hashes} from "@openzeppelin/contracts/utils/cryptography/Hashes.sol";

/**
 * @dev These functions deal with verification of Merkle Tree proofs.
 *
 *      The tree and the proofs can be generated using our
 *      https://docs.rs/binary-merkle-tree[Rust library].
 *      You will find a quickstart guide in the readme.
 *
 *      https://github.com/paritytech/polkadot-sdk/blob/master/substrate/utils/binary-merkle-tree/src/lib.rs
 */
library BinaryMerkleTree {
    /**
     * @dev Verifies a Merkle proof against a given root hash.
     *
     *      The proof is NOT expected to contain leaf hash as the first
     *      element, but only all adjacent nodes required to eventually by process of
     *      concatenating and hashing end up with given root hash.
     *
     *      The proof must not contain the root hash.
     *
     * @param root Root hash of the Merkle tree.
     * @param proof Merkle proof, which is an array of hashes.
     * @param numberOfLeaves Total number of leaves in the Merkle tree.
     * @param leafIndex Index of the leaf in the Merkle tree.
     * @param leafHash Hash of the leaf to verify.
     * @return isValid `true` if the proof is valid, `false` otherwise.
     */
    function verifyProofCalldata(
        bytes32 root,
        bytes32[] calldata proof,
        uint256 numberOfLeaves,
        uint256 leafIndex,
        bytes32 leafHash
    ) internal pure returns (bool) {
        if (leafIndex >= numberOfLeaves) {
            return false;
        }

        return processProofCalldata(proof, numberOfLeaves, leafIndex, leafHash) == root;
    }

    /**
     * @dev Processes a Merkle proof and returns the computed root hash.
     *
     * @param proof Merkle proof, which is an array of hashes.
     * @param numberOfLeaves Total number of leaves in the Merkle tree.
     * @param leafIndex Index of the leaf in the Merkle tree.
     * @param leafHash Hash of the leaf to verify.
     * @return computed Root hash of the Merkle tree.
     */
    function processProofCalldata(bytes32[] calldata proof, uint256 numberOfLeaves, uint256 leafIndex, bytes32 leafHash)
        internal
        pure
        returns (bytes32)
    {
        uint256 position = leafIndex;
        uint256 width = numberOfLeaves;
        bytes32 computed = leafHash;

        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 a = computed;
            bytes32 b = proof[i];

            uint256 positionPlusOne;
            unchecked {
                positionPlusOne = position + 1;
            }

            // TODO: consider optimizing this (use OpenZeppelin's `commutativeKeccak256` instead of `efficientKeccak256`).
            if (position % 2 == 1 || positionPlusOne == width) {
                computed = Hashes.efficientKeccak256(b, a);
            } else {
                computed = Hashes.efficientKeccak256(a, b);
            }

            position /= 2;
            unchecked {
                width = ((width - 1) / 2) + 1;
            }
        }

        return computed;
    }

    /**
     * @dev Verifies a Merkle proof against a given root hash.
     *
     *      The proof is NOT expected to contain leaf hash as the first
     *      element, but only all adjacent nodes required to eventually by process of
     *      concatenating and hashing end up with given root hash.
     *
     *      The proof must not contain the root hash.
     *
     * @param root Root hash of the Merkle tree.
     * @param proof Merkle proof, which is an array of hashes.
     * @param numberOfLeaves Total number of leaves in the Merkle tree.
     * @param leafIndex Index of the leaf in the Merkle tree.
     * @param leafHash Hash of the leaf to verify.
     * @return isValid `true` if the proof is valid, `false` otherwise.
     */
    function verifyProof(
        bytes32 root,
        bytes32[] memory proof,
        uint256 numberOfLeaves,
        uint256 leafIndex,
        bytes32 leafHash
    ) internal pure returns (bool) {
        if (leafIndex >= numberOfLeaves) {
            return false;
        }

        return processProof(proof, numberOfLeaves, leafIndex, leafHash) == root;
    }

    /**
     * @dev Processes a Merkle proof and returns the computed root hash.
     *
     * @param proof Merkle proof, which is an array of hashes.
     * @param numberOfLeaves Total number of leaves in the Merkle tree.
     * @param leafIndex Index of the leaf in the Merkle tree.
     * @param leafHash Hash of the leaf to verify.
     * @return computed Root hash of the Merkle tree.
     */
    function processProof(bytes32[] memory proof, uint256 numberOfLeaves, uint256 leafIndex, bytes32 leafHash)
        internal
        pure
        returns (bytes32)
    {
        uint256 position = leafIndex;
        uint256 width = numberOfLeaves;
        bytes32 computed = leafHash;

        for (uint256 i = 0; i < proof.length; i++) {
            bytes32 a = computed;
            bytes32 b = proof[i];

            uint256 positionPlusOne;
            unchecked {
                positionPlusOne = position + 1;
            }

            // TODO: consider optimizing this (use OpenZeppelin's `commutativeKeccak256` instead of `efficientKeccak256`).
            if (position % 2 == 1 || positionPlusOne == width) {
                computed = Hashes.efficientKeccak256(b, a);
            } else {
                computed = Hashes.efficientKeccak256(a, b);
            }

            position /= 2;
            unchecked {
                width = ((width - 1) / 2) + 1;
            }
        }

        return computed;
    }
}
