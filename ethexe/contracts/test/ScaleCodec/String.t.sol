// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestStringScaleCodec is Test {
    function test_stringEncode() public pure {
        assertEq(ScaleCodec.encodeString("hello"), hex"1468656c6c6f");
    }

    function test_stringDecode() public pure {
        assertEq(ScaleCodec.decodeString(hex"1468656c6c6f", 0).value, "hello");
    }
}
