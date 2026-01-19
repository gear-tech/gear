// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.28;
import {Test} from "forge-std/Test.sol";
import {Mirror} from "../src/Mirror.sol";
import {Gear} from "../src/libraries/Gear.sol";
import {console} from "forge-std/console.sol";

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
        uint256 gas = mirrorPerformStateTransitionGas(mockTransition);
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
}
