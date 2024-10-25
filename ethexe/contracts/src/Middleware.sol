// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.26;

import {Time} from "@openzeppelin/contracts/utils/types/Time.sol";

contract Middleware {
    uint48 public immutable ERA_DURATION;
    uint48 public immutable GENESIS_TIMESTAMP;
    address public immutable DELEGATOR_FACTORY;
    address public immutable OPERATOR_SPECIFIC_DELEGATOR_TYPE_INDEX;
    address public immutable SLASHER_FACTORY;
    address public immutable OPERATOR_REGISTRY;
    address public immutable NETWORK_REGISTRY;
    address public immutable COLLATERAL;

    constructor(
        uint48 eraDuration,
        address delegatorFactory,
        address operatorSpecificDelegatorTypeIndex,
        address slasherFactory,
        address operatorRegistry,
        address networkRegistry,
        address collateral
    ) {
        ERA_DURATION = eraDuration;
        GENESIS_TIMESTAMP = Time.timestamp();
        DELEGATOR_FACTORY = delegatorFactory;
        OPERATOR_SPECIFIC_DELEGATOR_TYPE_INDEX = operatorSpecificDelegatorTypeIndex;
        SLASHER_FACTORY = slasherFactory;
        OPERATOR_REGISTRY = operatorRegistry;
        NETWORK_REGISTRY = networkRegistry;
        COLLATERAL = collateral;
    }
}
