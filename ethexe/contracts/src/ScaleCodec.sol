// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

function bytesToUint(bytes memory data, uint256 byte_length, uint256 offset) pure returns (uint256) {
    uint256 result = 0;

    assembly {
        let data_ptr := add(data, 0x20)
        for { let i := offset } lt(i, byte_length) { i := add(i, 1) } {
            let byte_value := byte(0, mload(add(add(data_ptr, i), offset)))
            result := or(result, shl(mul(i, 8), byte_value))
        }
    }

    return result;
}

library ScaleCodec {
    struct CompactInt {
        uint256 value;
        uint256 offset;
    }

    struct DecodedString {
        string value;
        uint256 offset;
    }

    struct Optional {
        bool isSome;
        bytes value;
    }

    struct Result {
        bool isOk;
        bool isErr;
        bytes value;
    }

    function sliceBytes(bytes memory data, uint256 start, uint256 end) public pure returns (bytes memory) {
        bytes memory result = new bytes(end - start);

        for (uint256 i = 0; i < end - start; i++) {
            result[i] = data[i + start];
        }

        return result;
    }

    function concatBytes(bytes[] memory value) public pure returns (bytes memory) {
        if (value.length == 1) {
            return value[0];
        }

        bytes memory res;

        for (uint256 i = 0; i < value.length; i++) {
            res = bytes.concat(res, value[i]);
        }

        return res;
    }

    function bytes1Tobytes(bytes1 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(1);
        result[0] = value;
        return result;
    }

    function bytes2Tobytes(bytes2 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(2);
        result[0] = value[0];
        result[1] = value[1];
        return result;
    }

    function bytes4Tobytes(bytes4 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(4);
        for (uint256 i = 0; i < 4; i++) {
            result[i] = value[i];
        }
        return result;
    }

    function bytes8Tobytes(bytes8 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(8);
        for (uint256 i = 0; i < 8; i++) {
            result[i] = value[i];
        }
        return result;
    }

    function bytes16Tobytes(bytes16 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(16);
        for (uint256 i = 0; i < 16; i++) {
            result[i] = value[i];
        }
        return result;
    }

    function bytes32Tobytes(bytes32 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(32);
        for (uint256 i = 0; i < 32; i++) {
            result[i] = value[i];
        }
        return result;
    }

    function bytesToBytes32(bytes memory value) public pure returns (bytes32) {
        return bytes32(value);
    }

    function encodeBool(bool value) public pure returns (bytes memory) {
        bytes memory result = new bytes(1);
        if (value) {
            result[0] = 0x01;
        } else {
            result[0] = 0x00;
        }
        return result;
    }

    function decodeBool(bytes memory _bytes, uint256 offset) public pure returns (bool) {
        return _bytes[offset] == 0x01;
    }

    function encodeUint8(uint8 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(1);
        encodeUint8To(value, result, 0);
        return result;
    }

    function encodeUint8To(uint8 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, value)
        }
    }

    function decodeUint8(bytes memory _bytes, uint256 offset) public pure returns (uint8) {
        return uint8(_bytes[offset]);
    }

    function encodeInt8(int8 value) public pure returns (bytes memory) {
        return encodeUint8(uint8(value));
    }

    function encodeInt8To(int8 value, bytes memory destination, uint256 offset) public pure {
        encodeUint8To(uint8(value), destination, offset);
    }

    function decodeInt8(bytes memory _bytes, uint256 offset) public pure returns (int8) {
        return int8(uint8(_bytes[offset]));
    }

    function encodeUint16(uint16 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(2);
        encodeUint16To(value, result, 0);
        return result;
    }

    function encodeUint16To(uint16 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, and(value, 0xff))
            mstore8(add(dest, 1), shr(0x08, value))
        }
    }

    function decodeUint16(bytes memory _bytes, uint256 offset) public pure returns (uint16) {
        return uint16(bytesToUint(_bytes, 2, offset));
    }

    function encodeInt16(int16 value) public pure returns (bytes memory) {
        return encodeUint16(uint16(value));
    }

    function encodeInt16To(int16 value, bytes memory destination, uint256 offset) public pure {
        encodeUint16To(uint16(value), destination, offset);
    }

    function decodeInt16(bytes memory _bytes, uint256 offset) public pure returns (int16) {
        return int16(decodeUint16(_bytes, offset));
    }

    function encodeUint32(uint32 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(4);
        encodeUint32To(value, result, 0);
        return result;
    }

    function encodeUint32To(uint32 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, and(value, 0xff))
            mstore8(add(dest, 1), shr(0x08, value))
            mstore8(add(dest, 2), shr(0x10, value))
            mstore8(add(dest, 3), shr(0x18, value))
        }
    }

    function decodeUint32(bytes memory _bytes, uint256 offset) public pure returns (uint32) {
        return uint32(bytesToUint(_bytes, 4, offset));
    }

    function encodeInt32(int32 value) public pure returns (bytes memory) {
        return encodeUint32(uint32(value));
    }

    function encodeInt32To(int32 value, bytes memory destination, uint256 offset) public pure {
        encodeUint32To(uint32(value), destination, offset);
    }

    function decodeInt32(bytes memory _bytes, uint256 offset) public pure returns (int32) {
        return int32(decodeUint32(_bytes, offset));
    }

    function encodeUint64(uint64 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(8);
        encodeUint64To(value, result, 0);
        return result;
    }

    function encodeUint64To(uint64 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, and(value, 0xff))
            for { let i := 1 } lt(i, 8) { i := add(i, 1) } { mstore8(add(dest, i), shr(mul(i, 8), value)) }
        }
    }

    function decodeUint64(bytes memory _bytes, uint256 offset) public pure returns (uint64) {
        return uint64(bytesToUint(_bytes, 8, offset));
    }

    function encodeInt64(int64 value) public pure returns (bytes memory) {
        return encodeUint64(uint64(value));
    }

    function encodeInt64To(int64 value, bytes memory destination, uint256 offset) public pure {
        encodeUint64To(uint64(value), destination, offset);
    }

    function decodeInt64(bytes memory _bytes, uint256 offset) public pure returns (int64) {
        return int64(decodeUint64(_bytes, offset));
    }

    function encodeUint128(uint128 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(16);
        encodeUint128To(value, result, 0);
        return result;
    }

    function encodeUint128To(uint128 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, and(value, 0xff))
            for { let i := 1 } lt(i, 16) { i := add(i, 1) } { mstore8(add(dest, i), shr(mul(i, 8), value)) }
        }
    }

    function decodeUint128(bytes memory _bytes, uint256 offset) public pure returns (uint128) {
        return uint128(bytesToUint(_bytes, 16, offset));
    }

    function encodeInt128(int128 value) public pure returns (bytes memory) {
        return encodeUint128(uint128(value));
    }

    function encodeInt128To(int128 value, bytes memory destination, uint256 offset) public pure {
        encodeUint128To(uint128(value), destination, offset);
    }

    function decodeInt128(bytes memory _bytes, uint256 offset) public pure returns (int128) {
        return int128(decodeUint128(_bytes, offset));
    }

    function encodeUint256(uint256 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(32);
        encodeUint256To(value, result, 0);
        return result;
    }

    function encodeUint256To(uint256 value, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            mstore8(dest, and(value, 0xff))
            for { let i := 1 } lt(i, 32) { i := add(i, 1) } { mstore8(add(dest, i), shr(mul(i, 8), value)) }
        }
    }

    function decodeUint256(bytes memory _bytes, uint256 offset) public pure returns (uint256) {
        return bytesToUint(_bytes, 32, offset);
    }

    function encodeCompactInt(uint256 value) public pure returns (bytes memory) {
        uint8 bytesLen = compactIntLen(value);
        bytes memory result = new bytes(bytesLen);
        encodeCompactIntTo(value, bytesLen, result, 0);
        return result;
    }

    function compactIntLen(uint256 value) public pure returns (uint8) {
        if (value < 1 << 6) {
            return 1;
        } else if (value < 1 << 14) {
            return 2;
        } else if (value < 1 << 30) {
            return 4;
        } else {
            uint8 bytes_len = 1;
            assembly {
                let v := value
                for {} gt(v, 0) { v := shr(8, v) } { bytes_len := add(bytes_len, 1) }
                if gt(bytes_len, 32) { revert(0, 0) }
            }
            return bytes_len;
        }
    }

    function encodeCompactIntTo(uint256 value, uint8 bytesLen, bytes memory destination, uint256 offset) public pure {
        assembly {
            let dest := add(add(destination, 0x20), offset)
            if lt(value, shl(6, 1)) { mstore8(dest, shl(2, value)) }
            if and(lt(value, shl(14, 1)), iszero(lt(value, shl(6, 1)))) {
                let v := add(shl(2, value), 1)
                mstore8(dest, v)
                mstore8(add(dest, 1), shr(8, v))
            }
            if and(lt(value, shl(30, 1)), iszero(lt(value, shl(14, 1)))) {
                let v := add(shl(2, value), 2)
                mstore8(dest, v)
                mstore8(add(dest, 1), shr(8, v))
                mstore8(add(dest, 2), shr(16, v))
                mstore8(add(dest, 3), shr(24, v))
            }
            if iszero(lt(value, shl(30, 1))) {
                let bytes_len := sub(bytesLen, 1)
                let first_byte := add(shl(2, sub(bytes_len, 4)), 3)
                mstore8(dest, first_byte)
                for { let i := 0 } lt(i, bytes_len) { i := add(i, 1) } {
                    mstore8(add(dest, add(i, 1)), shr(mul(i, 8), value))
                }
            }
        }
    }

    function decodeCompactInt(bytes memory _bytes, uint256 offset) public pure returns (CompactInt memory) {
        uint8 mode = uint8(_bytes[offset]) & 0x03;

        if (mode == 0x00) {
            return CompactInt(uint8(_bytes[offset]) >> 2, 1);
        } else if (mode == 0x01) {
            uint16 _value;
            assembly {
                let src_ptr := add(add(_bytes, 0x20), offset)
                let v := byte(0, mload(add(src_ptr, 1)))
                _value := or(_value, shl(8, v))
                v := byte(0, mload(src_ptr))
                _value := or(_value, v)
            }
            return CompactInt(_value >> 2, 2);
        } else if (mode == 0x02) {
            uint32 _value;
            assembly {
                let src_ptr := add(add(_bytes, 0x20), offset)
                for { let i := 3 } gt(i, 0) { i := sub(i, 1) } {
                    let v := byte(0, mload(add(src_ptr, i)))
                    _value := or(_value, shl(mul(i, 8), v))
                }
                let v := byte(0, mload(src_ptr))
                _value := or(_value, v)

            }
            return CompactInt(_value >> 2, 4);
        } else {
            uint8 bytes_len = (uint8(_bytes[offset]) >> 2) + 4;

            uint8 size = 0;

            if (bytes_len <= 8) {
                size = 8;
            } else if (bytes_len <= 16) {
                size = 16;
            } else if (bytes_len <= 32) {
                size = 32;
            } else {
                size = 64;
            }

            bytes memory _result = new bytes(size);

            assembly {
                let res := add(_result, 0x20)
                let src_ptr := add(add(_bytes, 0x20), offset)

                for { let i := 0 } lt(i, size) { i := add(i, 1) } {
                    if lt(bytes_len, sub(size, i)) {
                        mstore8(add(res, i), 0x00)
                    }
                    if iszero(lt(bytes_len, sub(size, i))) {
                        let v := byte(0, mload(add(src_ptr, sub(size, i))))
                        mstore8(add(res, i), v)
                    }
                }
            }

            if (size == 8) {
                return CompactInt(uint64(bytes8(_result)), 8);
            } else if (size == 16) {
                return CompactInt(uint128(bytes16(_result)), 16);
            } else {
                return CompactInt(uint256(bytes32(_result)), 32);
            }
        }
    }

    function encodeString(string memory value) public pure returns (bytes memory) {
        bytes memory result = bytes(value);
        bytes memory len = encodeCompactInt(result.length);

        return bytes.concat(len, result);
    }

    function decodeString(bytes memory _bytes, uint256 offset) public pure returns (DecodedString memory) {
        CompactInt memory len = decodeCompactInt(_bytes, offset);

        offset += len.offset;

        bytes memory result = new bytes(len.value);
        for (uint256 i = 0; i < len.value; i++) {
            result[i] = _bytes[i + offset];
        }

        offset += len.value;

        return DecodedString(string(result), offset);
    }

    function encodeVec(bytes[] memory value) public pure returns (bytes memory) {
        bytes memory len = encodeCompactInt(value.length);
        uint256 total_len = len.length;

        for (uint256 i = 0; i < value.length; i++) {
            total_len += value[i].length;
        }

        bytes memory res = new bytes(total_len);

        for (uint256 i = 0; i < len.length; i++) {
            res[i] = len[i];
        }

        uint256 offset = len.length;

        for (uint256 i = 0; i < value.length; i++) {
            for (uint256 j = 0; j < value[i].length; j++) {
                res[offset + j] = value[i][j];
            }
            offset += value[i].length;
        }

        return res;
    }

    function decodeVec(bytes memory _bytes, uint256 item_len, bool unknown_len, uint256 offset)
        public
        pure
        returns (bytes[] memory)
    {
        CompactInt memory prefix = decodeCompactInt(_bytes, offset);
        bytes[] memory result = new bytes[](prefix.value);

        uint256 _offset = offset + prefix.offset;

        for (uint256 i = 0; i < prefix.value; i++) {
            uint256 item_prefix_len = 0;
            if (unknown_len) {
                CompactInt memory item_prefix = decodeCompactInt(_bytes, _offset);
                item_len = item_prefix.value;
                item_prefix_len = item_prefix.offset;
            }

            bytes memory item = new bytes(item_len + item_prefix_len);

            for (uint256 j = 0; j < item_len + item_prefix_len; j++) {
                item[j] = _bytes[_offset + j];
            }

            result[i] = item;
            _offset += item_len + item_prefix_len;

            if (_offset >= _bytes.length) {
                break;
            }
        }

        return result;
    }

    function encodeOptional(Optional memory value) public pure returns (bytes memory) {
        if (value.isSome) {
            bytes memory result = new bytes(value.value.length + 1);
            result[0] = 0x01;

            for (uint256 i = 0; i < value.value.length; i++) {
                result[i + 1] = value.value[i];
            }

            return result;
        } else {
            bytes memory result = new bytes(1);
            result[0] = 0x00;

            return result;
        }
    }

    function decodeOptional(bytes memory _bytes, uint256 offset) public pure returns (Optional memory) {
        if (_bytes[offset] == 0x00) {
            return Optional(false, new bytes(0));
        } else {
            return Optional(true, sliceBytes(_bytes, 1 + offset, _bytes.length));
        }
    }

    function encodeResult(Result memory value) public pure returns (bytes memory) {
        if (value.isOk) {
            return bytes.concat(hex"00", value.value);
        } else {
            return bytes.concat(hex"01", value.value);
        }
    }

    function decodeResult(bytes memory _bytes, uint256 offset) public pure returns (Result memory) {
        bytes memory value = sliceBytes(_bytes, 1 + offset, _bytes.length);
        if (_bytes[offset] == 0x00) {
            return Result(true, false, value);
        } else {
            return Result(false, true, value);
        }
    }
}
