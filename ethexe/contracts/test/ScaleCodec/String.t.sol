// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestStringScaleCodec is Test {
    function test_stringEncode() public pure {
        assertEq(ScaleCodec.encodeString("hello"), hex"1468656c6c6f");
    }

    function test_stringEncodeTo() public pure {
        bytes memory _bytes = new bytes(7);
        _bytes[0] = 0x01;

        string memory _str = "hello";
        uint256 strLen = ScaleCodec.stringLen(_str);
        uint8 strLenPrefixLen = ScaleCodec.compactIntLen(strLen);
        ScaleCodec.encodeCompactIntTo(strLen, strLenPrefixLen, _bytes, 1);

        ScaleCodec.encodeStringTo("hello", strLen, _bytes, 1 + strLenPrefixLen);
        assertEq(_bytes, hex"011468656c6c6f");
    }

    function test_stringDecode() public pure {
        assertEq(ScaleCodec.decodeString(hex"1468656c6c6f", 0).value, "hello");
    }

    function test_strLen() public pure {
        assertEq(ScaleCodec.stringLen("hello"), 5);
    }
}
