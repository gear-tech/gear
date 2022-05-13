use diesel::Queryable;

use super::schema::{tests, test_executions};

#[derive(Debug, Queryable)]
pub struct Test {
    pub id: i32,
    pub crate_name: String,
    pub test_name: String,
}

#[derive(Insertable)]
#[table_name="tests"]
pub struct NewTest<'a> {
    pub crate_name: &'a str,
    pub test_name: &'a str,
}

#[derive(Insertable)]
#[table_name="test_executions"]
pub struct NewTestExecution<'a> {
    pub test_id: i32,
    pub commit_hash: &'a str,
    pub date_time: i32,
    pub exec_time: i64,
}
