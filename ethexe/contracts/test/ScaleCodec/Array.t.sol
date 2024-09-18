// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.25;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestArrayScaleCodec is Test {
    function encodeArray(string[5] memory _value) internal pure returns (bytes memory) {
        bytes[] memory result = new bytes[](5);
        for (uint256 i = 0; i < 5; i++) {
            result[i] = ScaleCodec.encodeString(_value[i]);
        }

        return ScaleCodec.concatBytes(result);
    }

    function decodeArray(bytes memory _value) internal pure returns (string[5] memory) {
        string[] memory result = new string[](5);

        uint256 offset = 0;

        for (uint256 i = 0; i < 5; i++) {
            ScaleCodec.DecodedString memory item = ScaleCodec.decodeString(_value, offset);
            result[i] = item.value;
            offset = item.offset;
        }

        return [result[0], result[1], result[2], result[3], result[4]];
    }

    function test_arrayEncode() public pure {
        string[5] memory _array = ["Gear", "is", "awesome", "and", "cool"];

        bytes memory encoded = hex"10476561720869731c617765736f6d650c616e6410636f6f6c";

        assertEq(encodeArray(_array), encoded);
    }

    function test_arrayDecode() public pure {
        string[5] memory _array = ["Gear", "is", "awesome", "and", "cool"];

        bytes memory encoded = hex"10476561720869731c617765736f6d650c616e6410636f6f6c";

        string[5] memory decoded = decodeArray(encoded);

        assertEq(decoded[0], _array[0]);
        assertEq(decoded[1], _array[1]);
        assertEq(decoded[2], _array[2]);
        assertEq(decoded[3], _array[3]);
        assertEq(decoded[4], _array[4]);
    }
}
