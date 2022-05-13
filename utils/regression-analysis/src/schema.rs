table! {
    test_executions (id) {
        id -> BigInt,
        test_id -> BigInt,
        commit_hash -> Text,
        date_time -> Integer,
        exec_time -> BigInt,
    }
}

table! {
    tests (id) {
        id -> BigInt,
        crate_name -> Text,
        test_name -> Text,
    }
}

allow_tables_to_appear_in_same_query!(
    test_executions,
    tests,
);
