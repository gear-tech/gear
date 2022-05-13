use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::env::current_dir;

use clap::Parser;

#[macro_use]
extern crate diesel;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use serde::{Deserialize, Serialize};

use crate::models::{Test, NewTest, NewTestExecution};

mod schema;
mod models;

const CRATE_NAMES: [&str; 2] = ["pallet-gear", "pallet-gas"];

no_arg_sql_function!(last_insert_rowid, diesel::sql_types::Integer, "Represents the SQL last_insert_rowid() function");

#[derive(Debug, Parser)]
// #[clap(group(
//     ArgGroup::new("opposite")
//         // .required(true)
//         .conflicts_with("supported_crates_group")
//         .args(&["db", "input"]),
// ))]
pub struct Args {
    /// Path to the database with benchmarks.
    #[clap(long)]
    db: std::path::PathBuf,

    #[clap(long, group = "supported_crates_group")]
    supported_crates: bool,

    // TODO: commit and 'dry-run' option
}

fn main() {
    let args = Args::parse();
    if args.supported_crates {
        for crate_name in CRATE_NAMES {
            println!("{}", crate_name);
        }

        return;
    }

    let current_directory = current_dir().expect("failed to get current working directory");

    let database_url = args.db.as_path().to_str().expect("failed to get database url");
    let db_connection = SqliteConnection::establish(database_url)
        .expect("failed to open DB");

    process_jsons(db_connection, current_directory, &CRATE_NAMES);

    println!("Hello, world!");
}

fn process_jsons(connection: SqliteConnection, current_directory: PathBuf, crate_names: &[&str]) {
    for (crate_name, json_path) in crate_names.iter()
        .map(|&crate_name| {
            let mut p = current_directory.clone();
            p.push(crate_name);

            (crate_name, p)
        })
    {
        process_json(&connection, crate_name, &json_path);
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct TestExecTime {
    #[serde(rename = "type")]
    test_type: String,
    name: String,
    event: String,
    exec_time: f64,
}

fn process_json(connection: &SqliteConnection, crate_name: &str, json_path: &Path) {
    use crate::schema::tests::dsl as tests_dsl;

    for line in read_lines(json_path).expect(&format!("failed to read lines from '{}'", json_path.display())) {
        let line = match line {
            Err(_) => continue,
            Ok(l) => l,
        };

        let test: TestExecTime = match serde_json::from_str(&line) {
            Err(_) => continue,
            Ok(t) => t,
        };

        if test.test_type != "test" || test.event != "ok" {
            continue;
        }

        let test_id = tests_dsl::tests
            .filter(tests_dsl::crate_name.eq(crate_name))
            .filter(tests_dsl::test_name.eq(&test.name))
            .load::<Test>(connection);

        let test_id = if let Some(test) = test_id.ok().and_then(|mut v| v.pop()) {
            use crate::schema::test_executions;

            let count = diesel::dsl::sql::<diesel::sql_types::BigInt>("count(id)");
            let average = diesel::dsl::sql::<diesel::sql_types::BigInt>("avg(exec_time)");
            let min = diesel::dsl::sql::<diesel::sql_types::BigInt>("min(exec_time)");
            let max = diesel::dsl::sql::<diesel::sql_types::BigInt>("max(exec_time)");

            let result = test_executions::table
                .select((count, average, min, max))
                .filter(test_executions::dsl::test_id.eq(test.id))
                .get_result::<(i64, i64, i64, i64)>(connection)
                .expect("Failed to compose stats");

            let median = test_executions::table
                .select(test_executions::dsl::exec_time)
                .filter(test_executions::dsl::test_id.eq(test.id))
                .order_by(test_executions::dsl::exec_time)
                .limit(2 - result.0 % 2)
                .offset((result.0) / 2)
                .load::<i64>(connection)
                .expect("failed to select median");
            
            let median = if median.len() > 1 {
                median[0] / 2 + median[1] / 2 + median[0] % 2 + median[1] % 2
            } else {
                median[0]
            };
            
            println!("stats = {:?}, median = {}", result, median);

            test.id
        } else {
            let new_test = NewTest {
                crate_name,
                test_name: &test.name,
            };

            diesel::insert_into(crate::schema::tests::table)
                .values(new_test)
                .execute(connection)
                .expect("failed to insert new test");

            crate::schema::tests::table
                .select(last_insert_rowid)
                .load::<_>(connection)
                .expect("failed to obtain the last id")
                .pop()
                .unwrap()
        };
        
        let new_test_execution = NewTestExecution {
            test_id,
            commit_hash: "sldkfsd",
            date_time: 1_000_000,
            exec_time: (test.exec_time * 1_000_000_000.0) as i64,
        };

        diesel::insert_into(crate::schema::test_executions::table)
            .values(new_test_execution)
            .execute(connection)
            .expect("failed to insert new execution of a test");

        println!("test = {:?}, id = {:?}", test, test_id);
    }
}

// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
