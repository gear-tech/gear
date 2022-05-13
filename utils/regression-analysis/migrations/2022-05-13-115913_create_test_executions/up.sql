-- Your SQL goes here
CREATE TABLE test_executions (
  id BIGINT PRIMARY KEY NOT NULL,
  -- FK
  test_id BIGINT NOT NULL,
  commit_hash VARCHAR NOT NULL,
  date_time INTEGER NOT NULL,
  exec_time BIGINT NOT NULL
);

CREATE INDEX index_test_executions_test_id ON test_executions (test_id);
