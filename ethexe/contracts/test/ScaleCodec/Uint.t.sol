// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestUintScaleCodec is Test {
    function test_uint8EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeUint8(uint8(69)), hex"45");
        assertEq(ScaleCodec.decodeUint8(hex"45", 0), uint8(69));
    }

    function test_uint16EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeUint16(uint16(42)), hex"2a00");
        assertEq(ScaleCodec.decodeUint16(hex"2a00", 0), uint16(42));
    }

    function test_uint32EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeUint32(uint32(16777215)), hex"ffffff00");
        assertEq(ScaleCodec.decodeUint32(hex"ffffff00", 0), uint32(16777215));
    }

    function test_uint64EncodeDecode() public pure {
        assertEq(ScaleCodec.encodeUint64(uint64(18446744073709)), hex"edb5a0f7c6100000");
        assertEq(ScaleCodec.decodeUint64(hex"edb5a0f7c6100000", 0), uint64(18446744073709));
    }

    function test_uint128EncodeDecode() public pure {
        assertEq(
            ScaleCodec.encodeUint128(uint128(340282366920938463463374607431)), hex"4754408bb92ca5b509fa824b04000000"
        );
        assertEq(
            ScaleCodec.decodeUint128(hex"4754408bb92ca5b509fa824b04000000", 0), uint128(340282366920938463463374607431)
        );
    }

    function test_Uint256Encode() public pure {
        assertEq(
            ScaleCodec.encodeUint256(uint256(340282366920938463463374607431)),
            hex"4754408bb92ca5b509fa824b0400000000000000000000000000000000000000"
        );
    }

    function test_Uint256Decode() public pure {
        assertEq(
            ScaleCodec.decodeUint256(hex"4754408bb92ca5b509fa824b0400000000000000000000000000000000000000", 0),
            uint256(340282366920938463463374607431)
        );
    }

    function test_CompactIntEncode() public pure {
        assertEq(ScaleCodec.encodeCompactInt(0), hex"00");
        assertEq(ScaleCodec.encodeCompactInt(1), hex"04");
        assertEq(ScaleCodec.encodeCompactInt(42), hex"a8");
        assertEq(ScaleCodec.encodeCompactInt(69), hex"1501");
        assertEq(ScaleCodec.encodeCompactInt(65535), hex"feff0300");
        assertEq(ScaleCodec.encodeCompactInt(100000000000000), hex"0b00407a10f35a");
    }

    function test_CompactIntDecode() public pure {
        assertEq(ScaleCodec.decodeCompactInt(hex"00", 0).value, 0);
        assertEq(ScaleCodec.decodeCompactInt(hex"04", 0).value, 1);
        assertEq(ScaleCodec.decodeCompactInt(hex"a8", 0).value, 42);
        assertEq(ScaleCodec.decodeCompactInt(hex"1501", 0).value, 69);
        assertEq(ScaleCodec.decodeCompactInt(hex"feff0300", 0).value, 65535);
        assertEq(ScaleCodec.decodeCompactInt(hex"0b00407a10f35a", 0).value, 100000000000000);
    }
}
