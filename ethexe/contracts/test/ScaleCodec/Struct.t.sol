// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestStructScaleCodec is Test {
    struct MyStruct {
        string name;
        uint8 age;
    }

    function encodeMyStruct(string memory _name, uint8 _age) internal pure returns (bytes memory) {
        MyStruct memory myStruct = MyStruct(_name, _age);

        uint totalLen = 0;

        uint256 __nameLen = ScaleCodec.stringLen(myStruct.name);
        uint8 __namePrefixLen = ScaleCodec.compactIntLen(__nameLen);
        totalLen += __nameLen + __namePrefixLen;

        totalLen += 1;

        bytes memory _bytes = new bytes(totalLen);

        uint offset = 0;
        ScaleCodec.encodeCompactIntTo(__nameLen, __namePrefixLen, _bytes, 0);
        offset += __namePrefixLen;
        ScaleCodec.encodeStringTo(myStruct.name, __nameLen, _bytes, offset);
        offset += __nameLen;
        ScaleCodec.encodeUint8To(myStruct.age, _bytes, offset);

        return _bytes;
    }

    function decodeMyStruct(bytes memory _value) internal pure returns (MyStruct memory) {
        ScaleCodec.DecodedString memory name = ScaleCodec.decodeString(_value, 0);
        uint8 age = ScaleCodec.decodeUint8(_value, name.offset);

        return MyStruct(name.value, age);
    }

    function test_MyStructEncode() public pure {
        MyStruct memory _myStruct = MyStruct({name: "Gear", age: 3});

        assertEq(encodeMyStruct(_myStruct.name, _myStruct.age), hex"104765617203");
    }

    function test_MyStructEncodeDecode() public pure {
        MyStruct memory _decoded = decodeMyStruct(hex"104765617203");
        assertEq(_decoded.name, "Gear");
        assertEq(_decoded.age, 3);
    }
}
