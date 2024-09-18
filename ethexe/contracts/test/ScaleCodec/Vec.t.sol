// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestVecScaleCodec is Test {
    function encodeVecU8(uint8[] memory _value) internal pure returns (bytes memory) {
        bytes[] memory vec = new bytes[](_value.length);

        for (uint256 i = 0; i < _value.length; i++) {
            vec[i] = ScaleCodec.encodeUint8(_value[i]);
        }

        return ScaleCodec.encodeVec(vec);
    }

    function decodeVecU8(bytes memory _value) internal pure returns (uint8[] memory) {
        bytes[] memory vec = ScaleCodec.decodeVec(_value, 1, false, 0);

        uint8[] memory result = new uint8[](vec.length);

        for (uint256 i = 0; i < vec.length; i++) {
            result[i] = ScaleCodec.decodeUint8(vec[i], 0);
        }

        return result;
    }

    function encodeVecString(string[] memory _value) internal pure returns (bytes memory) {
        bytes[] memory vec = new bytes[](_value.length);

        for (uint256 i = 0; i < _value.length; i++) {
            vec[i] = ScaleCodec.encodeString(_value[i]);
        }

        return ScaleCodec.encodeVec(vec);
    }

    function decodeVecString(bytes memory _value) internal pure returns (string[] memory) {
        bytes[] memory vec = ScaleCodec.decodeVec(_value, 0, true, 0);

        string[] memory result = new string[](vec.length);

        for (uint256 i = 0; i < vec.length; i++) {
            result[i] = ScaleCodec.decodeString(vec[i], 0).value;
        }

        return result;
    }

    function encodeVecVecU16(uint16[][] memory _value) public pure returns (bytes memory) {
        bytes[] memory vec = new bytes[](_value.length);

        for (uint256 i = 0; i < _value.length; i++) {
            bytes[] memory inner_vec = new bytes[](_value[i].length);
            for (uint256 j = 0; j < _value[i].length; j++) {
                inner_vec[j] = ScaleCodec.encodeUint16(_value[i][j]);
            }
            vec[i] = ScaleCodec.encodeVec(inner_vec);
        }

        return ScaleCodec.encodeVec(vec);
    }

    function test_encodeVec() public pure {
        uint8[] memory _vecUint8 = new uint8[](3);
        _vecUint8[0] = 1;
        _vecUint8[1] = 2;
        _vecUint8[2] = 3;
        assertEq(encodeVecU8(_vecUint8), hex"0c010203");

        string[] memory _vecString = new string[](2);
        _vecString[0] = "hello";
        _vecString[1] = "world";
        assertEq(encodeVecString(_vecString), hex"081468656c6c6f14776f726c64");

        uint16[][] memory _vecVecUint16 = new uint16[][](2);
        _vecVecUint16[0] = new uint16[](3);
        _vecVecUint16[0][0] = 1;
        _vecVecUint16[0][1] = 2;
        _vecVecUint16[0][2] = 3;
        _vecVecUint16[1] = new uint16[](3);
        _vecVecUint16[1][0] = 100;
        _vecVecUint16[1][1] = 200;
        _vecVecUint16[1][2] = 300;
        assertEq(encodeVecVecU16(_vecVecUint16), hex"080c0100020003000c6400c8002c01");
    }

    function test_decodeVec() public pure {
        uint8[] memory _decodedUint8 = decodeVecU8(hex"0c010203");
        assertEq(_decodedUint8[0], 1);
        assertEq(_decodedUint8[1], 2);
        assertEq(_decodedUint8[2], 3);

        string[] memory _decodedString = decodeVecString(hex"081468656c6c6f14776f726c64");
        assertEq(_decodedString[0], "hello");
        assertEq(_decodedString[1], "world");
    }
}
