use std::collections::HashMap;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::env::assert_database_env;
use super::process::assert_command_exists;
use super::temp::epoch_nanos;

static TEST_DATABASE_COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct TestDatabase {
    name: String,
    pub url: String,
    drop_on_exit: bool,
    isolation: TestDatabaseIsolation,
}

enum TestDatabaseIsolation {
    Database { admin_url: String },
    Schema { base_url: String },
}

impl TestDatabase {
    pub fn create(env: &HashMap<String, String>) -> Self {
        let base_url = assert_database_env(env);
        let name = format!(
            "hinemos_test_{}_{}_{}",
            std::process::id(),
            epoch_nanos(),
            TEST_DATABASE_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let admin_url = database_url_with_name(&base_url, "postgres");
        let url = database_url_with_name(&base_url, &name);

        let createdb = Command::new("createdb")
            .args(["--maintenance-db", &admin_url, &name])
            .output();
        if let Ok(output) = &createdb
            && output.status.success()
        {
            return Self {
                name,
                url,
                drop_on_exit: true,
                isolation: TestDatabaseIsolation::Database { admin_url },
            };
        }

        let createdb_error = match createdb {
            Ok(output) => String::from_utf8_lossy(&output.stderr).into_owned(),
            Err(error) => error.to_string(),
        };
        assert_command_exists("psql");
        let schema_url = database_url_with_search_path(&base_url, &name);
        let create_schema = Command::new("psql")
            .args([
                &base_url,
                "--no-align",
                "--tuples-only",
                "--command",
                &format!("create schema {name};"),
            ])
            .output()
            .expect("spawn psql create schema");
        assert!(
            create_schema.status.success(),
            "failed to create isolated integration test database `{}`: {}\n\
             fallback schema creation also failed: {}",
            name,
            createdb_error,
            String::from_utf8_lossy(&create_schema.stderr)
        );

        Self {
            name,
            url: schema_url,
            drop_on_exit: true,
            isolation: TestDatabaseIsolation::Schema { base_url },
        }
    }

    pub fn query_value(&self, sql: &str) -> String {
        assert_command_exists("psql");
        let output = Command::new("psql")
            .args([&self.url, "--no-align", "--tuples-only", "--command", sql])
            .output()
            .expect("spawn psql");
        assert!(
            output.status.success(),
            "psql query failed: {}\nsql:\n{}",
            String::from_utf8_lossy(&output.stderr),
            sql
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        if !self.drop_on_exit
            || std::env::var("HINEMOS_VERIFY_KEEP_DB").ok().as_deref() == Some("1")
        {
            eprintln!("test database kept: {}", self.name);
            return;
        }
        match &self.isolation {
            TestDatabaseIsolation::Database { admin_url } => {
                let _ = Command::new("dropdb")
                    .args([
                        "--if-exists",
                        "--force",
                        "--maintenance-db",
                        admin_url,
                        &self.name,
                    ])
                    .status();
            }
            TestDatabaseIsolation::Schema { base_url } => {
                let _ = Command::new("psql")
                    .args([
                        base_url,
                        "--no-align",
                        "--tuples-only",
                        "--command",
                        &format!("drop schema if exists {} cascade;", self.name),
                    ])
                    .status();
            }
        }
    }
}

fn database_url_with_name(base_url: &str, database: &str) -> String {
    let (before_query, query) = base_url
        .split_once('?')
        .map_or((base_url, ""), |(before, query)| (before, query));
    let slash = before_query
        .rfind('/')
        .expect("DATABASE_URL must include a database path");
    let mut url = format!("{}/{}", &before_query[..slash], database);
    if !query.is_empty() {
        url.push('?');
        url.push_str(query);
    }
    url
}

fn database_url_with_search_path(base_url: &str, schema: &str) -> String {
    let separator = if base_url.contains('?') { '&' } else { '?' };
    format!("{base_url}{separator}options=-csearch_path%3D{schema}%2Cpublic")
}
