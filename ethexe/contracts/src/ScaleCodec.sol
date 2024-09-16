// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

function bytesToUint(bytes memory data, uint256 byte_length) pure returns (uint256) {
    uint256 result = 0;

    for (uint256 i = 0; i < byte_length; i++) {
        result = result | (uint256(uint8(data[i])) << (i * 8));
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

    function decodeBool(bytes memory _bytes) public pure returns (bool) {
        return _bytes[0] == 0x01;
    }

    function encodeUint8(uint8 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(1);
        result[0] = bytes1(value);
        return result;
    }

    function decodeUint8(bytes memory _bytes) public pure returns (uint8) {
        return uint8(_bytes[0]);
    }

    function encodeInt8(int8 value) public pure returns (bytes memory) {
        return encodeUint8(uint8(value));
    }

    function decodeInt8(bytes memory _bytes) public pure returns (int8) {
        return int8(uint8(_bytes[0]));
    }

    function encodeUint16(uint16 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(2);
        result[0] = bytes2(value)[1];
        result[1] = bytes2(value)[0];
        return result;
    }

    function decodeUint16(bytes memory _bytes) public pure returns (uint16) {
        return uint16(bytesToUint(_bytes, 2));
    }

    function encodeInt16(int16 value) public pure returns (bytes memory) {
        return encodeUint16(uint16(value));
    }

    function decodeInt16(bytes memory _bytes) public pure returns (int16) {
        return int16(decodeUint16(_bytes));
    }

    function encodeUint32(uint32 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(4);

        bytes4 _value = bytes4(value);

        for (uint8 i = 0; i < 4; i++) {
            result[i] = _value[3 - i];
        }
        return result;
    }

    function decodeUint32(bytes memory _bytes) public pure returns (uint32) {
        return uint32(bytesToUint(_bytes, 4));
    }

    function encodeInt32(int32 value) public pure returns (bytes memory) {
        return encodeUint32(uint32(value));
    }

    function decodeInt32(bytes memory _bytes) public pure returns (int32) {
        return int32(decodeUint32(_bytes));
    }

    function encodeUint64(uint64 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(8);
        bytes8 _value = bytes8(value);

        for (uint8 i = 0; i < 8; i++) {
            result[i] = _value[7 - i];
        }
        return result;
    }

    function decodeUint64(bytes memory _bytes) public pure returns (uint64) {
        return uint64(bytesToUint(_bytes, 8));
    }

    function encodeInt64(int64 value) public pure returns (bytes memory) {
        return encodeUint64(uint64(value));
    }

    function decodeInt64(bytes memory _bytes) public pure returns (int64) {
        return int64(decodeUint64(_bytes));
    }

    function encodeUint128(uint128 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(16);
        bytes16 _value = bytes16(value);
        for (uint8 i = 0; i < 16; i++) {
            result[i] = _value[15 - i];
        }
        return result;
    }

    function decodeUint128(bytes memory _bytes) public pure returns (uint128) {
        return uint128(bytesToUint(_bytes, 16));
    }

    function encodeInt128(int128 value) public pure returns (bytes memory) {
        return encodeUint128(uint128(value));
    }

    function decodeInt128(bytes memory _bytes) public pure returns (int128) {
        return int128(decodeUint128(_bytes));
    }

    function encodeUint256(uint256 value) public pure returns (bytes memory) {
        bytes memory result = new bytes(32);
        bytes32 _value = bytes32(value);
        for (uint8 i = 0; i < 32; i++) {
            result[i] = _value[31 - i];
        }
        return result;
    }

    function decodeUint256(bytes memory _bytes) public pure returns (uint256) {
        return bytesToUint(_bytes, 32);
    }

    function encodeCompactInt(uint256 value) public pure returns (bytes memory) {
        if (value < 1 << 6) {
            uint8 v = uint8(value << 2);
            bytes memory result = new bytes(1);
            result[0] = bytes1(v);
            return result;
        } else if (value < 1 << 14) {
            uint16 v = uint16((value << 2) + 1);
            bytes memory result = new bytes(2);
            result[0] = bytes2(v)[1];
            result[1] = bytes2(v)[0];
            return result;
        } else if (value < 1 << 30) {
            uint32 v = uint32((value << 2) + 2);
            bytes memory result = new bytes(4);
            result[0] = bytes4(v)[3];
            result[1] = bytes4(v)[2];
            result[2] = bytes4(v)[1];
            result[3] = bytes4(v)[0];
            return result;
        } else {
            bytes memory _value = new bytes(32);

            bytes32 v = bytes32(uint256(value));

            for (uint256 i = 0; i < 32; i++) {
                _value[i] = v[31 - i];
            }

            uint8 bytes_len = uint8(_value.length);

            while (_value[bytes_len - 1] == 0) {
                bytes_len--;
            }

            bytes1 _len = bytes1(((bytes_len - 4) << 2) + 3);

            bytes memory result = new bytes(bytes_len + 1);

            result[0] = _len;

            for (uint256 i = 0; i < bytes_len; i++) {
                result[i + 1] = _value[i];
            }

            return result;
        }
    }

    function decodeCompactInt(bytes memory _bytes) public pure returns (CompactInt memory) {
        uint8 mode = uint8(_bytes[0]) & 0x03;

        if (mode == 0x00) {
            return CompactInt(uint8(_bytes[0]) >> 2, 1);
        } else if (mode == 0x01) {
            bytes memory _value = new bytes(2);
            _value[0] = _bytes[1];
            _value[1] = _bytes[0];
            return CompactInt(uint16(bytes2(_value)) >> 2, 2);
        } else if (mode == 0x02) {
            bytes memory _value = new bytes(4);
            _value[0] = _bytes[3];
            _value[1] = _bytes[2];
            _value[2] = _bytes[1];
            _value[3] = _bytes[0];
            return CompactInt(uint32(bytes4(_value)) >> 2, 4);
        } else {
            uint8 bytes_len = (uint8(_bytes[0]) >> 2) + 4;

            bytes memory _value = new bytes(bytes_len + 1);

            for (uint256 i = 0; i < bytes_len + 1; i++) {
                _value[i] = _bytes[i];
            }

            if (bytes_len <= 8) {
                bytes_len = 8;
            } else if (bytes_len <= 16) {
                bytes_len = 16;
            } else if (bytes_len <= 32) {
                bytes_len = 32;
            } else {
                bytes_len = 64;
            }

            bytes memory _result = new bytes(bytes_len);

            for (uint256 i = 0; i < bytes_len - _value.length; i++) {
                _result[i] = bytes1(0);
            }

            for (uint256 i = bytes_len - _value.length + 1; i < bytes_len; i++) {
                _result[i] = _value[bytes_len - i];
            }

            if (bytes_len == 8) {
                return CompactInt(uint64(bytes8(_result)), 8);
            } else if (bytes_len == 16) {
                return CompactInt(uint128(bytes16(_result)), 16);
            } else {
                return CompactInt(uint256(bytes32(_result)), 32);
            }
        }
    }

    function encodeString(string memory value) public pure returns (bytes memory) {
        bytes memory result = bytes(value);
        bytes memory len = encodeCompactInt(result.length);

        bytes memory res = new bytes(len.length + result.length);

        for (uint256 i = 0; i < len.length; i++) {
            res[i] = len[i];
        }

        for (uint256 i = 0; i < result.length; i++) {
            res[i + len.length] = result[i];
        }

        return res;
    }

    function decodeString(bytes memory _bytes) public pure returns (DecodedString memory) {
        CompactInt memory len = decodeCompactInt(_bytes);
        bytes memory result = new bytes(len.value);

        for (uint256 i = 0; i < len.value; i++) {
            result[i] = _bytes[i + 1];
        }

        return DecodedString(string(result), len.offset + result.length);
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

    function decodeVec(bytes memory _bytes, uint256 item_len, bool unknown_len) public pure returns (bytes[] memory) {
        CompactInt memory prefix = decodeCompactInt(_bytes);
        bytes[] memory result = new bytes[](prefix.value);

        uint256 offset = prefix.offset;

        bytes memory _value = sliceBytes(_bytes, offset, _bytes.length);

        for (uint256 i = 0; i < prefix.value; i++) {
            uint256 item_prefix_len = 0;
            if (unknown_len) {
                // item_len = decodeCompactInt(value[offset]).len;
                CompactInt memory item_prefix = decodeCompactInt(_value);
                item_len = item_prefix.value;
                item_prefix_len = item_prefix.offset;
            }

            bytes memory item = new bytes(item_len + item_prefix_len);

            for (uint256 j = 0; j < item_len + item_prefix_len; j++) {
                item[j] = _bytes[offset + j];
            }

            result[i] = item;
            offset += item_len + item_prefix_len;

            if (offset >= _bytes.length) {
                break;
            }
            _value = sliceBytes(_bytes, offset, _bytes.length - 1);
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

    function decodeOptional(bytes memory _bytes) public pure returns (Optional memory) {
        if (_bytes[0] == 0x00) {
            return Optional(false, new bytes(0));
        } else {
            return Optional(true, sliceBytes(_bytes, 1, _bytes.length));
        }
    }

    function encodeResult(Result memory value) public pure returns (bytes memory) {
        bytes[] memory result = new bytes[](2);
        result[0] = new bytes(1);
        result[1] = value.value;
        if (value.isOk) {
            result[0][0] = 0x00;
        } else {
            result[0][0] = 0x01;
        }
        return concatBytes(result);
    }

    function decodeResult(bytes memory _bytes) public pure returns (Result memory) {
        bytes memory value = sliceBytes(_bytes, 1, _bytes.length);
        if (_bytes[0] == 0x00) {
            return Result(true, false, value);
        } else {
            return Result(false, true, value);
        }
    }
}
