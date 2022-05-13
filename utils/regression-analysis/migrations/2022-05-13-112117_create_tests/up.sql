-- Your SQL goes here
CREATE TABLE tests (
  id INTEGER PRIMARY KEY NOT NULL,
  crate_name VARCHAR NOT NULL,
  test_name VARCHAR NOT NULL
);

CREATE INDEX index_tests_crate_test ON tests (crate_name, test_name);
