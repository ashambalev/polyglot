use polyglot_sql::dialects::Dialect;
use polyglot_sql::{parse, DialectType};

fn assert_clickhouse_parse(sql: &str) {
    let parsed = parse(sql, DialectType::ClickHouse).expect("ClickHouse SQL should parse");
    assert_eq!(parsed.len(), 1);
}

fn normalize_clickhouse(sql: &str) -> String {
    let dialect = Dialect::get(DialectType::ClickHouse);
    let parsed = dialect.parse(sql).expect("ClickHouse SQL should parse");
    let statement = parsed.first().expect("expected a statement");
    let transformed = dialect
        .transform(statement.clone())
        .expect("ClickHouse transform should succeed");
    dialect
        .generate_with_source(&transformed, DialectType::ClickHouse)
        .expect("ClickHouse generation should succeed")
}

fn assert_clickhouse_normalized_roundtrip(sql: &str) {
    let first = normalize_clickhouse(sql);
    let second = normalize_clickhouse(&first);

    assert_eq!(second, first);
}

#[test]
fn clickhouse_parses_array_literal_with_nested_subscript_and_cast() {
    assert_clickhouse_parse("SELECT [[1][1]]::Array(UInt32)");
}

#[test]
fn clickhouse_parses_array_literal_elements_with_type_casts() {
    assert_clickhouse_parse("SELECT [1::UInt32, 2::UInt32]::Array(UInt64)");
    assert_clickhouse_parse("SELECT [[1, 2]::Array(UInt32), [3]]::Array(Array(UInt64))");
}

#[test]
fn clickhouse_parses_nested_array_literal_subscript_chain() {
    assert_clickhouse_parse(
        "SELECT [[10, 2, 13, 15][toNullable(toLowCardinality(1))]][materialize(toLowCardinality(1))]",
    );
}

#[test]
fn clickhouse_parses_array_literal_followed_by_multiple_casts() {
    assert_clickhouse_parse("SELECT [[[1, 2, 3]::Array(UInt64)::Dynamic]]");
}

#[test]
fn clickhouse_recovers_terminal_unterminated_string_probe() {
    assert_eq!(
        normalize_clickhouse("select 'select json"),
        "SELECT 'select json'"
    );
}

#[test]
fn clickhouse_normalizes_ctas_without_repeated_parentheses() {
    let sql = "CREATE TABLE x ENGINE=Memory AS (SELECT 1)";
    let first = normalize_clickhouse(sql);
    let second = normalize_clickhouse(&first);

    assert_eq!(first, "CREATE TABLE x ENGINE=Memory AS (SELECT 1)");
    assert_eq!(second, first);
}

#[test]
fn clickhouse_preserves_table_function_ctas_source() {
    assert_eq!(
        normalize_clickhouse("CREATE TABLE t AS numbers(5)"),
        "CREATE TABLE t AS numbers(5)"
    );
}

#[test]
fn clickhouse_bare_with_totals_stays_parseable() {
    assert_eq!(
        normalize_clickhouse("SELECT count() WITH TOTALS"),
        "SELECT count() WITH TOTALS"
    );
}

#[test]
fn clickhouse_preserves_quoted_dotted_array_join_alias() {
    assert_eq!(
        normalize_clickhouse("SELECT x FROM t ARRAY JOIN s.a AS `s.a`"),
        "SELECT x FROM t ARRAY JOIN s.a AS \"s.a\""
    );
}

#[test]
fn clickhouse_preserves_quoted_database_names() {
    assert_eq!(
        normalize_clickhouse("CREATE DATABASE `this.is.a.valid.databasename`"),
        "CREATE DATABASE \"this.is.a.valid.databasename\""
    );
}

#[test]
fn clickhouse_preserves_incomplete_insert_probe_as_command() {
    assert_eq!(normalize_clickhouse("INSERT INTO t0"), "INSERT INTO t0");
}

#[test]
fn clickhouse_recovers_missing_terminal_rparen_for_extracted_subquery() {
    assert_eq!(
        normalize_clickhouse("SELECT count() FROM (SELECT 1"),
        "SELECT count() FROM (SELECT 1)"
    );
}

#[test]
fn clickhouse_preserves_sample_clause_keyword() {
    assert_eq!(
        normalize_clickhouse("SELECT count() FROM t SAMPLE 0.1"),
        "SELECT count() FROM t SAMPLE 0.1"
    );
}

#[test]
fn clickhouse_ttl_set_clause_stays_stable() {
    let sql = "CREATE TABLE t (key Int, date Date, value String) ENGINE = MergeTree() ORDER BY key TTL date + INTERVAL 2 MONTH GROUP BY key SET value = argMax(value, date)";

    assert_clickhouse_normalized_roundtrip(sql);
    assert_eq!(
        normalize_clickhouse(sql),
        "CREATE TABLE t (key Int32, date DATE, value String) ENGINE=MergeTree() ORDER BY key TTL date + INTERVAL '2' MONTH GROUP BY key SET value = argMax(value, date)"
    );
}

#[test]
fn clickhouse_preserves_enum8_custom_type_name() {
    assert_eq!(
        normalize_clickhouse("SELECT CAST(x AS Enum8('hello' = -123, 'world'))"),
        "SELECT CAST(x AS Enum8('hello' = -123, 'world'))"
    );
}

#[test]
fn clickhouse_enum8_custom_type_suffix_escapes_control_characters() {
    let sql = "SELECT CAST(x AS Enum8('a\tb' = 1, 'c\\\\d' = 2))";

    assert_clickhouse_normalized_roundtrip(sql);
}

#[test]
fn clickhouse_preserves_partial_with_probe_as_command() {
    assert_eq!(normalize_clickhouse("WITH build AS ("), "WITH build AS (");
}

#[test]
fn clickhouse_preserves_alter_update_mutation_as_command() {
    assert_eq!(
        normalize_clickhouse("ALTER TABLE tab UPDATE str = 'I am not inverted' WHERE 1"),
        "ALTER TABLE tab UPDATE str = 'I am not inverted' WHERE 1"
    );
}
