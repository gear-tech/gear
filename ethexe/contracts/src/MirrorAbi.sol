// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.26;

import {Proxy} from "@openzeppelin/contracts/proxy/Proxy.sol";
import {IMirror} from "./IMirror.sol";
import {Gear} from "./libraries/Gear.sol";
import {IRouter} from "./IRouter.sol";
import {IWrappedVara} from "./IWrappedVara.sol";

contract MirrorAbi is IMirror, Proxy {
    bytes32 public stateHash;
    address public inheritor;
    /// @dev This nonce is the source for message ids unique generations.
    /// Must be bumped on each send.
    /// Zeroed nonce is always represent init message by eligible account.
    uint256 public nonce;
    address public router;
    address public initializer;
    address public implAddress;
    address private _abi;
    bytes32 private constant PROXY_SLOT = 0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc;

    function initialize(address _initializer, address _router, address _implAddress, address _interface) public {
        require(initializer == address(0), "initializer could only be set once");

        initializer = _initializer;
        router = _router;
        implAddress = _implAddress;
        _abi = _interface;

        assembly ("memory-safe") {
            sstore(PROXY_SLOT, _interface)
        }
    }

    function _delegate(address) internal override {
        uint256 len = msg.data.length;
        require(len >= 32, "Mirror: invalid message data length");
        uint128 value = uint128(uint256(bytes32(msg.data[len - 32])));

        bytes memory payload = abi.encodeWithSelector(IMirror.sendMessage.selector, msg.data, value);

        (bool success,) = implAddress.delegatecall(payload);

        require(success, "Mirror: delegatecall failed");
    }

    function _implementation() internal view override returns (address) {
        return _abi;
    }

    function _mirrorImplDelegatecall() private returns (bytes memory resultData) {
        assembly ("memory-safe") {
            let freeMemPtr := mload(0x40)
            let implAddr := sload(implAddress.slot)

            calldatacopy(0, 0, calldatasize())

            let result := delegatecall(gas(), implAddr, 0, calldatasize(), 0, 0)

            if eq(result, 0) { revert(0, returndatasize()) }

            let returnSize := returndatasize()

            mstore(0x40, add(freeMemPtr, returnSize))

            returndatacopy(freeMemPtr, 0, returnSize)

            resultData := freeMemPtr
        }
    }

    /// @dev Only the router can call functions marked with this modifier.
    modifier onlyRouter() {
        require(msg.sender == router, "caller is not the router");
        _;
    }

    /// @dev Non-zero value must be transferred from source to router in functions marked with this modifier.
    modifier retrievingValue(uint128 value) {
        if (value != 0) {
            address routerAddr = router;
            bool success = _wvara(routerAddr).transferFrom(msg.sender, routerAddr, value);
            require(success, "failed to transfer non-zero amount of WVara from source to router");
        }
        _;
    }

    // TODO (breathx): terminated programs compute threshold must always be treated as balance-enough.
    /// @dev Functions marked with this modifier can be called only after the program is terminated.
    modifier whenTerminated() {
        require(inheritor != address(0), "program is not terminated");
        _;
    }

    /// @dev Functions marked with this modifier can be called only after the initializer has created the init message.
    modifier whenInitMessageCreated() {
        require(nonce > 0, "initializer hasn't created init message yet");
        _;
    }

    /// @dev Functions marked with this modifier can be called only after the initializer has created the init message or from the initializer (first access).
    modifier whenInitMessageCreatedOrFromInitializer() {
        require(
            nonce > 0 || msg.sender == initializer,
            "initializer hasn't created init message yet; and source is not initializer"
        );
        _;
    }

    /// @dev Functions marked with this modifier can be called only if the program is active.
    modifier whileActive() {
        require(inheritor == address(0), "program is terminated");
        _;
    }

    function sendMessage(bytes calldata, /*_payload*/ uint128 _value)
        external
        whileActive
        whenInitMessageCreatedOrFromInitializer
        retrievingValue(_value)
        returns (bytes32 resultData)
    {
        bytes memory result = _mirrorImplDelegatecall();

        assembly ("memory-safe") {
            resultData := mload(result)
        }
    }

    function sendReply(bytes32, /*_repliedTo*/ bytes calldata, /*_payload*/ uint128 _value)
        external
        whileActive
        whenInitMessageCreated
        retrievingValue(_value)
    {
        _mirrorImplDelegatecall();
    }

    function claimValue(bytes32 /*_claimedId*/ ) external whenInitMessageCreated {
        _mirrorImplDelegatecall();
    }

    function executableBalanceTopUp(uint128 _value) external whileActive retrievingValue(_value) {
        _mirrorImplDelegatecall();
    }

    function transferLockedValueToInheritor() public whenTerminated {
        _mirrorImplDelegatecall();
    }

    // NOTE (breathx): value to receive should be already handled in router.
    function performStateTransition(Gear.StateTransition calldata /*_transition*/ )
        external
        onlyRouter
        returns (bytes32 resultData)
    {
        bytes memory result = _mirrorImplDelegatecall();

        assembly ("memory-safe") {
            resultData := mload(result)
        }
    }

    receive() external payable {
        revert();
    }

    function _wvara(address routerAddr) private view returns (IWrappedVara) {
        address wvaraAddr = IRouter(routerAddr).wrappedVara();
        return IWrappedVara(wvaraAddr);
    }
}
