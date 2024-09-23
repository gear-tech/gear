// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestUintScaleCodec is Test {
    function test_uint8Encode() public pure {
        assertEq(ScaleCodec.encodeUint8(uint8(69)), hex"45");

        // Encode to
        bytes memory _bytes = new bytes(2);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint8To(uint8(69), _bytes, 1);
        assertEq(_bytes, hex"0145");
    }

    function test_uint8Decode() public pure {
        assertEq(ScaleCodec.decodeUint8(hex"45", 0), uint8(69));
        assertEq(ScaleCodec.decodeUint8(hex"0145", 1), uint8(69));
    }

    function test_uint16Encode() public {
        assertEq(ScaleCodec.encodeUint16(uint16(42)), hex"2a00");

        // Encode to
        bytes memory _bytes = new bytes(3);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint16To(uint16(42), _bytes, 1);
        assertEq(_bytes, hex"012a00");
    }

    function test_uint16Decode() public pure {
        assertEq(ScaleCodec.decodeUint16(hex"2a00", 0), uint16(42));
        assertEq(ScaleCodec.decodeUint16(hex"002a00", 1), uint16(42));
    }

    function test_uint32Encode() public pure {
        assertEq(ScaleCodec.encodeUint32(uint32(16777215)), hex"ffffff00");

        // Encode to
        bytes memory _bytes = new bytes(5);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint32To(uint32(16777215), _bytes, 1);
        assertEq(_bytes, hex"01ffffff00");
    }

    function test_uint32Decode() public pure {
        assertEq(ScaleCodec.decodeUint32(hex"ffffff00", 0), uint32(16777215));
        assertEq(ScaleCodec.decodeUint32(hex"00ffffff00", 1), uint32(16777215));
    }

    function test_uint64Encode() public pure {
        assertEq(ScaleCodec.encodeUint64(uint64(18446744073709)), hex"edb5a0f7c6100000");

        // Encode to
        bytes memory _bytes = new bytes(9);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint64To(uint64(18446744073709), _bytes, 1);
        assertEq(_bytes, hex"01edb5a0f7c6100000");
    }

    function test_uint64Decode() public pure {
        assertEq(ScaleCodec.decodeUint64(hex"edb5a0f7c6100000", 0), uint64(18446744073709));
        assertEq(ScaleCodec.decodeUint64(hex"02edb5a0f7c6100000", 1), uint64(18446744073709));
    }

    function test_uint128Encode() public pure {
        assertEq(
            ScaleCodec.encodeUint128(uint128(340282366920938463463374607431)), hex"4754408bb92ca5b509fa824b04000000"
        );

        // Encode to
        bytes memory _bytes = new bytes(17);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint128To(uint128(340282366920938463463374607431), _bytes, 1);
        assertEq(_bytes, hex"014754408bb92ca5b509fa824b04000000");
    }

    function test_uint128Decode() public pure {
        assertEq(
            ScaleCodec.decodeUint128(hex"4754408bb92ca5b509fa824b04000000", 0), uint128(340282366920938463463374607431)
        );
        assertEq(
            ScaleCodec.decodeUint128(hex"014754408bb92ca5b509fa824b04000000", 1),
            uint128(340282366920938463463374607431)
        );
    }

    function test_Uint256Encode() public pure {
        assertEq(
            ScaleCodec.encodeUint256(uint256(340282366920938463463374607431)),
            hex"4754408bb92ca5b509fa824b0400000000000000000000000000000000000000"
        );

        // Encode to
        bytes memory _bytes = new bytes(33);
        _bytes[0] = 0x01;
        ScaleCodec.encodeUint256To(uint256(340282366920938463463374607431), _bytes, 1);
        assertEq(_bytes, hex"014754408bb92ca5b509fa824b0400000000000000000000000000000000000000");
    }

    function test_Uint256Decode() public pure {
        assertEq(
            ScaleCodec.decodeUint256(hex"4754408bb92ca5b509fa824b0400000000000000000000000000000000000000", 0),
            uint256(340282366920938463463374607431)
        );
        assertEq(
            ScaleCodec.decodeUint256(hex"014754408bb92ca5b509fa824b0400000000000000000000000000000000000000", 1),
            uint256(340282366920938463463374607431)
        );
    }

    function test_CompactIntEncode() public pure {
        assertEq(ScaleCodec.encodeCompactInt(0), hex"00");
        assertEq(ScaleCodec.encodeCompactInt(1), hex"04");
        assertEq(ScaleCodec.encodeCompactInt(42), hex"a8");
        assertEq(ScaleCodec.encodeCompactInt(69), hex"1501");
        assertEq(ScaleCodec.encodeCompactInt(65535), hex"feff0300");
        assertEq(ScaleCodec.encodeCompactInt(1073741824), hex"0300000040");
        assertEq(ScaleCodec.encodeCompactInt(1000000000000), hex"070010a5d4e8");
        assertEq(ScaleCodec.encodeCompactInt(100000000000000), hex"0b00407a10f35a");
    }

    function test_CompactIntEncodeTo() public pure {
        bytes memory result = new bytes(2);
        result[0] = 0x01;
        ScaleCodec.encodeCompactIntTo(1, 1, result, 1);
        assertEq(result, hex"0104");
    }

    function test_CompactIntDecode() public pure {
        assertEq(ScaleCodec.decodeCompactInt(hex"00", 0).value, 0);
        assertEq(ScaleCodec.decodeCompactInt(hex"04", 0).value, 1);
        assertEq(ScaleCodec.decodeCompactInt(hex"01a8", 1).value, 42);
        assertEq(ScaleCodec.decodeCompactInt(hex"1501", 0).value, 69);
        assertEq(ScaleCodec.decodeCompactInt(hex"feff0300", 0).value, 65535);
        assertEq(ScaleCodec.decodeCompactInt(hex"0b00407a10f35a", 0).value, 100000000000000);

        ScaleCodec.CompactInt memory value = ScaleCodec.decodeCompactInt(hex"010b00407a10f35a", 1);
        assertEq(value.value, 100000000000000);
        assertEq(value.offset, 7);

        value = ScaleCodec.decodeCompactInt(hex"01070010a5d4e8", 1);
        assertEq(value.value, 1000000000000);
        assertEq(value.offset, 6);

        value = ScaleCodec.decodeCompactInt(hex"010300000040", 1);
        assertEq(value.value, 1073741824);
        assertEq(value.offset, 5);
    }

    function test_CompactLen() public pure {
        assertEq(ScaleCodec.compactIntLen(0), 1);
        assertEq(ScaleCodec.compactIntLen(69), 2);
        assertEq(ScaleCodec.compactIntLen(665535), 4);
        assertEq(ScaleCodec.compactIntLen(100000000000000), 7);
    }
}
