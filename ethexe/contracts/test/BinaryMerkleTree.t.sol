// Copyright (C) Gear Technologies Inc.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0
pragma solidity ^0.8.35;

import {Test} from "forge-std/Test.sol";
import {BinaryMerkleTree} from "src/libraries/BinaryMerkleTree.sol";

contract BinaryMerkleTreeWrapper {
    function verifyProofCalldata(
        bytes32 root,
        bytes32[] calldata proof,
        uint256 numberOfLeaves,
        uint256 leafIndex,
        bytes32 leafHash
    ) external pure returns (bool) {
        return BinaryMerkleTree.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash);
    }

    function verifyProof(
        bytes32 root,
        bytes32[] memory proof,
        uint256 numberOfLeaves,
        uint256 leafIndex,
        bytes32 leafHash
    ) external pure returns (bool) {
        return BinaryMerkleTree.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash);
    }
}

contract BinaryMerkleTreeTest is Test {
    BinaryMerkleTreeWrapper public binaryMerkleTreeWrapper;

    function setUp() public {
        binaryMerkleTreeWrapper = new BinaryMerkleTreeWrapper();
    }

    function test_VerifyProofCalldata() public view {
        // []
        bytes32 root = 0;
        bytes32[] memory proof = new bytes32[](0);
        uint256 numberOfLeaves = 0;
        uint256 leafIndex = 0;
        bytes32 leafHash = 0;
        assertFalse(binaryMerkleTreeWrapper.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32])]
        root = 0x1111111111111111111111111111111111111111111111111111111111111111;
        proof = new bytes32[](0);
        numberOfLeaves = 1;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32])]
        root = 0x3e92e0db88d6afea9edc4eedf62fffa4d92bcdfc310dccbe943747fe8302e871;
        proof = new bytes32[](1);
        proof[0] = 0x2222222222222222222222222222222222222222222222222222222222222222;
        numberOfLeaves = 2;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32])]
        root = 0x3e92e0db88d6afea9edc4eedf62fffa4d92bcdfc310dccbe943747fe8302e871;
        proof = new bytes32[](1);
        proof[0] = 0x1111111111111111111111111111111111111111111111111111111111111111;
        numberOfLeaves = 2;
        leafIndex = 1;
        leafHash = 0x2222222222222222222222222222222222222222222222222222222222222222;
        assertTrue(binaryMerkleTreeWrapper.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32]), H256::from([0x33; 32])]
        root = 0x54cc47e0e0577877f9bdb2727df082c5f7a97451f4dd1695cbbfde937f4376c4;
        proof = new bytes32[](2);
        proof[0] = 0x2222222222222222222222222222222222222222222222222222222222222222;
        proof[1] = 0x3333333333333333333333333333333333333333333333333333333333333333;
        numberOfLeaves = 3;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProofCalldata(root, proof, numberOfLeaves, leafIndex, leafHash));
    }

    function test_VerifyProof() public view {
        // []
        bytes32 root = 0;
        bytes32[] memory proof = new bytes32[](0);
        uint256 numberOfLeaves = 0;
        uint256 leafIndex = 0;
        bytes32 leafHash = 0;
        assertFalse(binaryMerkleTreeWrapper.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32])]
        root = 0x1111111111111111111111111111111111111111111111111111111111111111;
        proof = new bytes32[](0);
        numberOfLeaves = 1;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32])]
        root = 0x3e92e0db88d6afea9edc4eedf62fffa4d92bcdfc310dccbe943747fe8302e871;
        proof = new bytes32[](1);
        proof[0] = 0x2222222222222222222222222222222222222222222222222222222222222222;
        numberOfLeaves = 2;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32])]
        root = 0x3e92e0db88d6afea9edc4eedf62fffa4d92bcdfc310dccbe943747fe8302e871;
        proof = new bytes32[](1);
        proof[0] = 0x1111111111111111111111111111111111111111111111111111111111111111;
        numberOfLeaves = 2;
        leafIndex = 1;
        leafHash = 0x2222222222222222222222222222222222222222222222222222222222222222;
        assertTrue(binaryMerkleTreeWrapper.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash));

        // [H256::from([0x11; 32]), H256::from([0x22; 32]), H256::from([0x33; 32])]
        root = 0x54cc47e0e0577877f9bdb2727df082c5f7a97451f4dd1695cbbfde937f4376c4;
        proof = new bytes32[](2);
        proof[0] = 0x2222222222222222222222222222222222222222222222222222222222222222;
        proof[1] = 0x3333333333333333333333333333333333333333333333333333333333333333;
        numberOfLeaves = 3;
        leafIndex = 0;
        leafHash = 0x1111111111111111111111111111111111111111111111111111111111111111;
        assertTrue(binaryMerkleTreeWrapper.verifyProof(root, proof, numberOfLeaves, leafIndex, leafHash));
    }
}
