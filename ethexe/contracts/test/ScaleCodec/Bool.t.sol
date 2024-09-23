// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestBoolScaleCodec is Test {
    function test_boolEncodeTrue() public pure {
        assertEq(ScaleCodec.encodeBool(true), hex"01");
    }

    function test_boolEncodeFalse() public pure {
        assertEq(ScaleCodec.encodeBool(false), hex"00");
    }

    function test_boolDecodeTrue() public pure {
        assertEq(ScaleCodec.decodeBool(hex"01", 0), true);
    }

    function test_boolDecodeFalse() public pure {
        assertEq(ScaleCodec.decodeBool(hex"00", 0), false);
    }
}
