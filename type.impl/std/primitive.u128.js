(function() {
    var type_impls = Object.fromEntries([["gcore",[]],["gear_core",[]],["gtest",[]],["pallet_gear_gas",[]]]);
    if (window.register_type_impls) {
        window.register_type_impls(type_impls);
    } else {
        window.pending_type_impls = type_impls;
    }
})()
//{"start":55,"fragment_lengths":[12,17,13,23]}