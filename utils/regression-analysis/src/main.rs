use std::fs::File;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::env::current_dir;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};

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

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run process. Collect statistics of the available executions and
    /// append new data
    Run {
        #[clap(long)]
        db: PathBuf,
        #[clap(long)]
        commit: String,
    },
    /// Returns the list of crates
    GetCrateList,
}

fn current_time_since_epoch_secs() -> i32 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i32
}

fn main() {
    let (db, commit) = match Cli::parse().command {
        Commands::GetCrateList => {
            for crate_name in CRATE_NAMES {
                println!("{}", crate_name);
            }

            return;
        },
        Commands::Run { db, commit } => (db, commit),
    };

    let current_directory = current_dir().expect("failed to get current working directory");

    let database_url = db.as_path().to_str().expect("failed to get database url");
    let db_connection = SqliteConnection::establish(database_url)
        .expect("failed to open DB");

    let result = process_jsons(db_connection, current_directory, &CRATE_NAMES, &commit, current_time_since_epoch_secs());
    for (name, stats) in result {
        println!("name = {}", name);
        let table = tabled::Table::new(stats);
        println!("{}", table);
        println!("");
    }
}

fn process_jsons<'a>(connection: SqliteConnection, current_directory: PathBuf, crate_names: &[&'a str], commit: &str, current_time: i32) -> BTreeMap<&'a str, Vec<TestStats>> {
    let mut result = BTreeMap::new();
    for (crate_name, json_path) in crate_names.iter()
        .map(|&crate_name| {
            let mut p = current_directory.clone();
            p.push(crate_name);

            (crate_name, p)
        })
    {
        let stats = process_json(&connection, crate_name, commit, &json_path, current_time);
        result.insert(crate_name, stats);
    }

    result
}

#[derive(Deserialize, Serialize, Debug)]
struct TestExecTime {
    #[serde(rename = "type")]
    test_type: String,
    name: String,
    event: String,
    exec_time: f64,
}

struct TestStats {
    name: String,
    average: i64,
    min: i64,
    max: i64,
    median: i64,
    current: i64,
}

impl tabled::Tabled for TestStats {
    const LENGTH: usize = 6;

    fn fields(&self) -> Vec<String> {
        let comparing_display = |value: i64| {
            let current = self.current;
            let sign: &str = if current == value {
                "=="
            } else if current > value {
                ">"
            } else {
                "<"
            };

            format!("{} ({})", value, sign)
        };

        vec![self.name.clone(), self.current.to_string(), comparing_display(self.median), comparing_display(self.average), comparing_display(self.min), comparing_display(self.max)]
    }

    fn headers() -> Vec<String> {
        vec!["name".to_owned(), "current".to_owned(), "median".to_owned(), "average".to_owned(), "min".to_owned(), "max".to_owned()]
    }
}

fn process_test(connection: &SqliteConnection, crate_name: &str, test_exec_time: &TestExecTime) -> (i32, Option<TestStats>) {
    use crate::schema::tests::dsl as tests_dsl;

    let test_id = tests_dsl::tests
        .filter(tests_dsl::crate_name.eq(crate_name))
        .filter(tests_dsl::test_name.eq(&test_exec_time.name))
        .load::<Test>(connection);

    if let Some(test) = test_id.ok().and_then(|mut v| v.pop()) {
        use crate::schema::test_executions;

        let sql_count = diesel::dsl::sql::<diesel::sql_types::BigInt>("count(id)");
        let sql_average = diesel::dsl::sql::<diesel::sql_types::BigInt>("avg(exec_time)");
        let sql_min = diesel::dsl::sql::<diesel::sql_types::BigInt>("min(exec_time)");
        let sql_max = diesel::dsl::sql::<diesel::sql_types::BigInt>("max(exec_time)");

        let (count, average, min, max) = test_executions::table
            .select((sql_count, sql_average, sql_min, sql_max))
            .filter(test_executions::dsl::test_id.eq(test.id))
            .get_result::<(i64, i64, i64, i64)>(connection)
            .expect("Failed to compose stats");

        let query = test_executions::table
            .select(test_executions::dsl::exec_time)
            .filter(test_executions::dsl::test_id.eq(test.id))
            .order_by(test_executions::dsl::exec_time)
            .limit(2 - count % 2)
            .offset((count - 1) / 2);
        // println!("{}", diesel::debug_query::<diesel::sqlite::Sqlite, _>(&query));
        let median = query
            .load::<i64>(connection)
            .expect("failed to select median");

        let median = if median.len() > 1 {
            median[0] / 2 + median[1] / 2 + median[0] % 2 + median[1] % 2
        } else {
            median[0]
        };

        (test.id, Some(TestStats{
            name: test.test_name,
            average,
            min,
            max,
            median,
            current: (1_000_000_000.0 * test_exec_time.exec_time) as i64,
        }))
    } else {
        let new_test = NewTest {
            crate_name,
            test_name: &test_exec_time.name,
        };

        diesel::insert_into(crate::schema::tests::table)
            .values(new_test)
            .execute(connection)
            .expect("failed to insert new test");

        (crate::schema::tests::table
            .select(last_insert_rowid)
            .load::<_>(connection)
            .expect("failed to obtain the last id")
            .pop()
            .unwrap(), None)
    }
}

fn process_json(connection: &SqliteConnection, crate_name: &str, commit: &str, json_path: &Path, current_time: i32) -> Vec<TestStats> {
    let mut result = Vec::with_capacity(1_000);

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

        let (test_id, stats) = process_test(connection, crate_name, &test);
        if let Some(stats) = stats {
            result.push(stats);
        }
        
        let new_test_execution = NewTestExecution {
            test_id,
            commit_hash: commit,
            date_time: current_time,
            exec_time: (test.exec_time * 1_000_000_000.0) as i64,
        };

        diesel::insert_into(crate::schema::test_executions::table)
            .values(new_test_execution)
            .execute(connection)
            .expect("failed to insert new execution of a test");
    }

    result
}

// Returns an Iterator to the Reader of the lines of the file.
fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
