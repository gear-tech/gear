// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestOptionalScaleCodec is Test {
    struct OptionalString {
        bool isSome;
        string value;
    }

    function encodeOptionalString(OptionalString memory _value) internal pure returns (bytes memory) {
        return ScaleCodec.encodeOptional(
            ScaleCodec.Optional({
                isSome: _value.isSome,
                value: _value.isSome ? ScaleCodec.encodeString(_value.value) : new bytes(0)
            })
        );
    }

    function decodeOptionalString(bytes memory _bytes) internal pure returns (OptionalString memory) {
        ScaleCodec.Optional memory decoded = ScaleCodec.decodeOptional(_bytes, 0);

        return OptionalString({
            isSome: decoded.isSome,
            value: decoded.isSome ? ScaleCodec.decodeString(decoded.value, 0).value : ""
        });
    }

    function test_OptionalNoneEncodeDecode() public pure {
        OptionalString memory _optional = OptionalString({isSome: false, value: ""});

        assertEq(encodeOptionalString(_optional), hex"00");

        OptionalString memory _decoded = decodeOptionalString(hex"00");

        assertEq(_decoded.isSome, false);
    }

    function test_OptionalSomeEncodeDecode() public pure {
        OptionalString memory _optional = OptionalString({isSome: true, value: "Gear"});

        assertEq(encodeOptionalString(_optional), hex"011047656172");

        OptionalString memory _decoded = decodeOptionalString(hex"011047656172");

        assertEq(_decoded.isSome, true);
        assertEq(_decoded.value, "Gear");
    }
}
