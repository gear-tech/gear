// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestIntScaleCodec is Test {
    function test_int8EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeInt8(int8(69)), hex"45");
        assertEq(ScaleCodec.decodeInt8(hex"45", 0), int8(69));

        assertEq(ScaleCodec.encodeInt8(int8(-69)), hex"bb");
        assertEq(ScaleCodec.decodeInt8(hex"bb", 0), int8(-69));
    }

    function test_int16EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeInt16(int16(42)), hex"2a00");
        assertEq(ScaleCodec.decodeInt16(hex"2a00", 0), int16(42));

        assertEq(ScaleCodec.encodeInt16(int16(-42)), hex"d6ff");
        assertEq(ScaleCodec.decodeInt16(hex"d6ff", 0), int16(-42));
    }

    function test_int32EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeInt32(int32(16777215)), hex"ffffff00");
        assertEq(ScaleCodec.decodeInt32(hex"ffffff00", 0), int32(16777215));

        assertEq(ScaleCodec.encodeInt32(int32(-16777215)), hex"010000ff");
        assertEq(ScaleCodec.decodeInt32(hex"010000ff", 0), int32(-16777215));
    }

    function test_int64EncodeDecode18446744073709() public pure {
        assertEq(ScaleCodec.encodeInt64(int64(18446744073709)), hex"edb5a0f7c6100000");
        assertEq(ScaleCodec.decodeInt64(hex"edb5a0f7c6100000", 0), int64(18446744073709));

        assertEq(ScaleCodec.encodeInt64(int64(-18446744073709)), hex"134a5f0839efffff");
        assertEq(ScaleCodec.decodeInt64(hex"134a5f0839efffff", 0), int64(-18446744073709));
    }

    function test_int128EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeInt128(int128(340282366920938463463374607431)), hex"4754408bb92ca5b509fa824b04000000");
        assertEq(
            ScaleCodec.decodeInt128(hex"4754408bb92ca5b509fa824b04000000", 0), int128(340282366920938463463374607431)
        );

        assertEq(
            ScaleCodec.encodeInt128(int128(-340282366920938463463374607431)), hex"b9abbf7446d35a4af6057db4fbffffff"
        );
        assertEq(
            ScaleCodec.decodeInt128(hex"b9abbf7446d35a4af6057db4fbffffff", 0), int128(-340282366920938463463374607431)
        );
    }
}
