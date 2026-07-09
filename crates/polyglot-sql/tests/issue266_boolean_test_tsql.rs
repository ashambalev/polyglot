use polyglot_sql::{Dialect, DialectType, TranspileOptions};

fn pg_to_target(sql: &str, target: DialectType) -> String {
    Dialect::get(DialectType::PostgreSQL)
        .transpile_with(sql, target, TranspileOptions::strict())
        .unwrap_or_else(|err| panic!("transpile failed for {sql:?} to {target:?}: {err}"))
        .into_iter()
        .next()
        .expect("expected one generated statement")
}

#[test]
fn negated_boolean_tests_preserve_null_rows_in_predicate_context() {
    let cases = [
        (
            "SELECT d FROM t WHERE b IS NOT FALSE",
            "SELECT d FROM t WHERE b = 1 OR b IS NULL",
        ),
        (
            "SELECT d FROM t WHERE b IS NOT TRUE",
            "SELECT d FROM t WHERE b = 0 OR b IS NULL",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}

#[test]
fn non_negated_boolean_tests_remain_simple_predicates() {
    let cases = [
        (
            "SELECT d FROM t WHERE b IS TRUE",
            "SELECT d FROM t WHERE b = 1",
        ),
        (
            "SELECT d FROM t WHERE b IS FALSE",
            "SELECT d FROM t WHERE b = 0",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}

#[test]
fn scalar_boolean_tests_are_definite_bit_values() {
    let cases = [
        (
            "SELECT b IS TRUE AS istrue FROM t",
            "SELECT CAST(CASE WHEN b = 1 THEN 1 ELSE 0 END AS BIT) AS istrue FROM t",
        ),
        (
            "SELECT b IS FALSE AS isfalse FROM t",
            "SELECT CAST(CASE WHEN b = 0 THEN 1 ELSE 0 END AS BIT) AS isfalse FROM t",
        ),
        (
            "SELECT b IS NOT TRUE AS isnt FROM t",
            "SELECT CAST(CASE WHEN b = 0 OR b IS NULL THEN 1 ELSE 0 END AS BIT) AS isnt FROM t",
        ),
        (
            "SELECT b IS NOT FALSE AS isnf FROM t",
            "SELECT CAST(CASE WHEN b = 1 OR b IS NULL THEN 1 ELSE 0 END AS BIT) AS isnf FROM t",
        ),
        (
            "SELECT b IS UNKNOWN AS isu FROM t",
            "SELECT CAST(CASE WHEN b IS NULL THEN 1 ELSE 0 END AS BIT) AS isu FROM t",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}

#[test]
fn boolean_tests_on_predicate_operands_do_not_compare_predicates_to_integers() {
    let cases = [
        (
            "SELECT d FROM t WHERE (a > 1) IS NOT FALSE",
            "SELECT d FROM t WHERE CASE WHEN NOT (a > 1) THEN 0 ELSE 1 END = 1",
        ),
        (
            "SELECT d FROM t WHERE (a > 1) IS NOT TRUE",
            "SELECT d FROM t WHERE CASE WHEN (a > 1) THEN 0 ELSE 1 END = 1",
        ),
        (
            "SELECT (a > 1) IS TRUE AS ok FROM t",
            "SELECT CAST(CASE WHEN (a > 1) THEN 1 ELSE 0 END AS BIT) AS ok FROM t",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}
