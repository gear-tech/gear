// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestBytesScaleCodec is Test {
    function test_bytesToBytes32() public pure {
        assertEq(
            ScaleCodec.bytesToBytes32(hex"050102030401020304010203040102030401020304010203040102030401020304", 1),
            hex"0102030401020304010203040102030401020304010203040102030401020304"
        );
    }

    function test_insertBytes32() public pure {
        bytes memory _bytes = new bytes(33);
        _bytes[0] = 0x05;
        ScaleCodec.insertBytes32To(hex"0102030401020304010203040102030401020304010203040102030401020304", _bytes, 1);
        assertEq(_bytes, hex"050102030401020304010203040102030401020304010203040102030401020304");
    }

    function test_insertBytes20() public pure {
        bytes memory _bytes = new bytes(21);
        _bytes[0] = 0x05;
        ScaleCodec.insertBytes20To(hex"0102030401020304010203040102030401020304", _bytes, 1);
        assertEq(_bytes, hex"050102030401020304010203040102030401020304");
    }
}
