// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.19;

import {ScaleCodec} from "src/ScaleCodec.sol";
import "forge-std/Test.sol";

contract TestResultScaleCodec is Test {
    struct ResultStringU8 {
        bool isOk;
        bool isErr;
        string ok;
        uint8 err;
    }

    function encodeResultStringU8(ResultStringU8 memory _value) internal pure returns (bytes memory) {
        if (_value.isOk) {
            return ScaleCodec.encodeResult(
                ScaleCodec.Result({isOk: true, isErr: false, value: ScaleCodec.encodeString(_value.ok)})
            );
        } else {
            return ScaleCodec.encodeResult(
                ScaleCodec.Result({isOk: false, isErr: true, value: ScaleCodec.encodeUint8(_value.err)})
            );
        }
    }

    function decodeResultStringU8(bytes memory _value) public pure returns (ResultStringU8 memory) {
        ScaleCodec.Result memory decoded = ScaleCodec.decodeResult(_value);

        if (decoded.isOk) {
            return ResultStringU8({isOk: true, isErr: false, ok: ScaleCodec.decodeString(decoded.value).value, err: 0});
        } else {
            return ResultStringU8({isOk: false, isErr: true, ok: "", err: ScaleCodec.decodeUint8(decoded.value)});
        }
    }

    function test_ResultOkEncodeDecode() public pure {
        ResultStringU8 memory _result = ResultStringU8({isOk: true, isErr: false, ok: "Gear", err: 0});

        assertEq(encodeResultStringU8(_result), hex"001047656172");

        ResultStringU8 memory _decoded = decodeResultStringU8(hex"001047656172");

        assertEq(_decoded.isOk, true);
        assertEq(_decoded.ok, "Gear");
    }

    function test_ResultErrEncodeDecode() public pure {
        ResultStringU8 memory _result = ResultStringU8({isOk: false, isErr: true, ok: "", err: 1});

        assertEq(encodeResultStringU8(_result), hex"0101");

        ResultStringU8 memory _decoded = decodeResultStringU8(hex"0101");

        assertEq(_decoded.isErr, true);
        assertEq(_decoded.err, 1);
    }
}
