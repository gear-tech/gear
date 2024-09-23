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

        bytes[] memory encoded_items = new bytes[](2);

        encoded_items[0] = ScaleCodec.encodeString(myStruct.name);
        encoded_items[1] = ScaleCodec.encodeUint8(myStruct.age);

        return ScaleCodec.concatBytes(encoded_items);
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
