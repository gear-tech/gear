table! {
    test_executions (id) {
        id -> Integer,
        test_id -> Integer,
        commit_hash -> Text,
        date_time -> Integer,
        exec_time -> BigInt,
    }
}

table! {
    tests (id) {
        id -> Integer,
        crate_name -> Text,
        test_name -> Text,
    }
}

allow_tables_to_appear_in_same_query!(
    test_executions,
    tests,
);
