// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestVecScaleCodec is Test {
    function test_encodeVecUint8() public pure {
        uint8[] memory data = new uint8[](3);
        data[0] = 1;
        data[1] = 2;
        data[2] = 3;

        uint8 vecPrefixLen = ScaleCodec.compactIntLen(data.length);
        uint256 totalLen = data.length + vecPrefixLen;

        bytes memory _bytes = new bytes(totalLen);

        uint256 offset = 0;

        ScaleCodec.encodeCompactIntTo(data.length, vecPrefixLen, _bytes, offset);

        offset += vecPrefixLen;

        for (uint256 i = 0; i < data.length; i++) {
            ScaleCodec.encodeUint8To(data[i], _bytes, offset);
            offset += 1;
        }

        assertEq(_bytes, hex"0c010203");
    }

    function test_encodeVecString() public pure {
        string[] memory data = new string[](2);
        data[0] = "hello";
        data[1] = "world";

        uint256 bytesLen = 0;

        uint8 vecPrefixLen = ScaleCodec.compactIntLen(data.length);

        for (uint256 i = 0; i < data.length; i++) {
            uint256 strLen = ScaleCodec.stringLen(data[i]);
            bytesLen += strLen;
            bytesLen += ScaleCodec.compactIntLen(strLen);
        }

        bytes memory _bytes = new bytes(bytesLen + vecPrefixLen);

        uint256 offset = 0;

        ScaleCodec.encodeCompactIntTo(data.length, vecPrefixLen, _bytes, offset);
        offset += vecPrefixLen;

        for (uint256 i = 0; i < data.length; i++) {
            uint256 strLen = ScaleCodec.stringLen(data[i]);
            uint256 prefixLen = ScaleCodec.compactIntLen(strLen);
            ScaleCodec.encodeCompactIntTo(strLen, vecPrefixLen, _bytes, offset);
            offset += prefixLen;
            ScaleCodec.encodeStringTo(data[i], strLen, _bytes, offset);
            offset += strLen;
        }

        assertEq(_bytes, hex"081468656c6c6f14776f726c64");
    }

    function test_decodeVecUint8() public pure {
        bytes memory data = hex"0c010203";

        ScaleCodec.CompactUint256 memory vecLen = ScaleCodec.decodeCompactInt(data, 0);
        uint256 offset = vecLen.offset;

        uint8[] memory vec = new uint8[](vecLen.value);

        for (uint256 i = 0; i < vec.length; i++) {
            vec[i] = ScaleCodec.decodeUint8(data, offset);
            offset++;
        }

        assertEq(vec[0], 1);
        assertEq(vec[1], 2);
        assertEq(vec[2], 3);
    }

    function test_decodeVecString() public pure {
        bytes memory data = hex"081468656c6c6f14776f726c64";

        ScaleCodec.CompactUint256 memory vecLen = ScaleCodec.decodeCompactInt(data, 0);
        uint256 offset = vecLen.offset;

        string[] memory vec = new string[](vecLen.value);

        for (uint256 i = 0; i < vec.length; i++) {
            ScaleCodec.DecodedString memory str = ScaleCodec.decodeString(data, offset);
            offset = str.offset;
            vec[i] = str.value;
        }

        assertEq(vec[0], "hello");
        assertEq(vec[1], "world");
    }
}
