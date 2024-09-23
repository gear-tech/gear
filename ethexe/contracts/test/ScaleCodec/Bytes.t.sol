// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestBytesScaleCodec is Test {
    function test_bytesToBytes32() public pure {
        assertEq(
            ScaleCodec.bytesToBytes32(hex"00000000000000000000000000000000000000000000000000000000000000000000"),
            hex"0000000000000000000000000000000000000000000000000000000000000000"
        );
    }
}
