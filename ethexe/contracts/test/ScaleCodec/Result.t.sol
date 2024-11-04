// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestResultScaleCodec is Test {
    struct ResultStringU8 {
        bool isOk;
        string ok;
        uint8 err;
    }

    function encodeResultStringU8(ResultStringU8 memory _value) internal pure returns (bytes memory) {
        if (_value.isOk) {
            return ScaleCodec.encodeResult(ScaleCodec.Result({isOk: true, value: ScaleCodec.encodeString(_value.ok)}));
        } else {
            return ScaleCodec.encodeResult(ScaleCodec.Result({isOk: false, value: ScaleCodec.encodeUint8(_value.err)}));
        }
    }

    function decodeResultStringU8(bytes memory _value) public pure returns (ResultStringU8 memory) {
        ScaleCodec.Result memory decoded = ScaleCodec.decodeResult(_value, 0);

        if (decoded.isOk) {
            return ResultStringU8({isOk: true, ok: ScaleCodec.decodeString(decoded.value, 0).value, err: 0});
        } else {
            return ResultStringU8({isOk: false, ok: "", err: ScaleCodec.decodeUint8(decoded.value, 0)});
        }
    }

    function test_ResultOkEncodeDecode() public pure {
        ResultStringU8 memory _result = ResultStringU8({isOk: true, ok: "Gear", err: 0});

        assertEq(encodeResultStringU8(_result), hex"001047656172");

        ResultStringU8 memory _decoded = decodeResultStringU8(hex"001047656172");

        assertEq(_decoded.isOk, true);
        assertEq(_decoded.ok, "Gear");
    }

    function test_ResultErrEncodeDecode() public pure {
        ResultStringU8 memory _result = ResultStringU8({isOk: false, ok: "", err: 1});

        assertEq(encodeResultStringU8(_result), hex"0101");

        ResultStringU8 memory _decoded = decodeResultStringU8(hex"0101");

        assertEq(_decoded.isOk, false);
        assertEq(_decoded.err, 1);
    }
}
