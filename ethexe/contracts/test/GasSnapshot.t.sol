// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
import {Test} from "forge-std/Test.sol";
import {Mirror} from "../src/Mirror.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {console} from "forge-std/console.sol";
import {Hashes} from "frost-secp256k1-evm/utils/cryptography/Hashes.sol";
import {Memory} from "frost-secp256k1-evm/utils/Memory.sol";

contract GasShapshotTest is Test {
    address routerAddress = address(1);
    Mirror mirror;

    function setUp() public {
        mirror = new Mirror(routerAddress);
        vm.startPrank(routerAddress);
        mirror.initialize(address(0), address(0), false);
        vm.stopPrank();
    }

    function testMeasureMirrorGasTransition() public {
        vm.startPrank(routerAddress);
        Gear.StateTransition memory mockTransition = Gear.StateTransition({
            actorId: address(mirror),
            newStateHash: bytes32("0x"),
            exited: false,
            inheritor: address(0),
            valueToReceive: uint128(0),
            valueToReceiveNegativeSign: false,
            valueClaims: new Gear.ValueClaim[](0),
            messages: new Gear.Message[](0)
        });
        uint256 gas = mirrorPerformStateTransitionGas(mockTransition, "mock transition gas");
        console.log("gas", gas);

        vm.stopPrank();
    }

    function mirrorPerformStateTransitionGas(Gear.StateTransition memory _transition, string memory label)
        public
        returns (uint256 stateTransitionGas)
    {
        vm.startSnapshotGas(label);
        mirror.performStateTransition(_transition);
        stateTransitionGas = vm.stopSnapshotGas();
    }

    ////////////////////////////////////////////////
    // Benchmarking Router commitBatch
    ////////////////////////////////////////////////
    function testRouterCommitMockBatch() public {
        Gear.BatchCommitment memory mockBatch = mockBatchCommitment();

    }

    function mockBatchCommitment() public view returns (Gear.BatchCommitment memory mockBatch) {
        uint256 ts = block.timestamp;
        mockBatch = Gear.BatchCommitment({
            blockHash: blockhash(block.number),
            blockTimestamp: uint48(block.timestamp),
            previousCommittedBatchHash: bytes32(0),
            expiry: uint8(1),
            chainCommitment: new Gear.ChainCommitment[](0),
            codeCommitments: new Gear.CodeCommitment[](0),
            rewardsCommitment: new Gear.RewardsCommitment[](0),
            validatorsCommitment: new Gear.ValidatorsCommitment[](0)
        });
    }

    ////////////////////////////////////////////////
    // Benchmarking memory allocation strategies
    ////////////////////////////////////////////////

    function testMemoryAllocationStrategies() public {
        vm.startSnapshotGas("one memory allocation");
        bytes32 oneAllocHash = oneMemoryAllocation();
        uint256 oneAllocGas = vm.stopSnapshotGas();

        vm.startSnapshotGas("multiple memory allocations");
        bytes32 multipleAllocHash = multipleMemoryAllocation();
        uint256 multipleAllocGas = vm.stopSnapshotGas();

        console.log("multipleAllocGas", multipleAllocGas);
        console.log("oneAllocGas", oneAllocGas);

        require(multipleAllocHash == oneAllocHash, "Hashes do not match");
    }

    function testOneBytesConcat() public pure returns (bytes32) {
        bytes memory data;
        bytes32 h = keccak256(abi.encodePacked(uint256(1)));
        data = bytes.concat(data, h);
        return keccak256(data);
    }

    function multipleMemoryAllocation() public pure returns (bytes32) {
        uint256 len = 100;
        bytes memory hashes;
        for (uint256 i = 0; i < len; i++) {
            bytes32 hash = keccak256(abi.encodePacked(i));
            hashes = bytes.concat(hashes, hash);
        }
        return keccak256(hashes);
    }

    function oneMemoryAllocation() public pure returns (bytes32) {
        uint256 len = 100;
        uint256 hashesMemPtr = Memory.allocate(len * 32);
        uint256 offset = 0;
        for (uint256 i = 0; i < len; i++) {
            bytes32 hash = keccak256(abi.encodePacked(i));
            Memory.writeWord(hashesMemPtr, offset, uint256(hash));
            offset += 32;
        }
        return bytes32(Hashes.efficientKeccak256(hashesMemPtr, 0, len * 32));
    }
}
