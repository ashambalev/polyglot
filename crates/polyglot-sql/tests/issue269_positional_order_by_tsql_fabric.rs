use polyglot_sql::{Dialect, DialectType, TranspileOptions};

fn pg_to_target(
    sql: &str,
    target: DialectType,
    options: TranspileOptions,
) -> polyglot_sql::Result<String> {
    Ok(Dialect::get(DialectType::PostgreSQL)
        .transpile_with(sql, target, options)?
        .into_iter()
        .next()
        .expect("expected one generated statement"))
}

#[test]
fn default_transpile_preserves_positional_order_by_for_tsql_and_fabric() {
    let cases = [
        ("SELECT f1 FROM t ORDER BY 1", "SELECT f1 FROM t ORDER BY 1"),
        (
            "SELECT f1 FROM t ORDER BY 1 DESC",
            "SELECT f1 FROM t ORDER BY 1 DESC",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            let result = pg_to_target(sql, target, TranspileOptions::default())
                .unwrap_or_else(|err| panic!("default {target:?} transpile failed: {err}"));
            assert_eq!(result, expected, "failed for {target:?}: {sql}");
        }
    }
}

#[test]
fn default_transpile_preserves_positional_order_by_on_set_operations_for_tsql_and_fabric() {
    for target in [DialectType::TSQL, DialectType::Fabric] {
        let result = pg_to_target(
            "SELECT f1 FROM a UNION SELECT f1 FROM b ORDER BY 1",
            target,
            TranspileOptions::default(),
        )
        .unwrap_or_else(|err| panic!("default {target:?} transpile failed: {err}"));

        assert!(
            result.ends_with("ORDER BY 1"),
            "set operation should preserve positional ORDER BY for {target:?}: {result}"
        );
        assert!(
            !result.contains("CASE WHEN 1 IS NULL"),
            "set operation should not emit a constant NULL-ordering CASE for {target:?}: {result}"
        );
    }
}

#[test]
fn strict_transpile_rejects_positional_order_by_null_ordering_for_tsql_and_fabric() {
    let cases = [
        ("SELECT f1 FROM t ORDER BY 1", "NULLS LAST"),
        ("SELECT f1 FROM t ORDER BY 1 DESC", "NULLS FIRST"),
        (
            "SELECT f1 FROM a UNION SELECT f1 FROM b ORDER BY 1",
            "NULLS LAST",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected_nulls_order) in cases {
            let err = pg_to_target(sql, target, TranspileOptions::strict())
                .expect_err("strict transpile should reject positional null-ordering simulation");
            let message = err.to_string();
            assert!(
                message.contains(expected_nulls_order) && message.contains("positional ordering"),
                "unexpected error for {target:?} {sql:?}: {message}"
            );
        }
    }
}

#[test]
fn strict_transpile_keeps_named_order_by_null_ordering_rewrite_for_tsql_and_fabric() {
    for target in [DialectType::TSQL, DialectType::Fabric] {
        let result = pg_to_target(
            "SELECT f1 FROM t ORDER BY f1",
            target,
            TranspileOptions::strict(),
        )
        .unwrap_or_else(|err| panic!("strict {target:?} named ORDER BY should work: {err}"));

        assert_eq!(
            result, "SELECT f1 FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1",
            "failed for {target:?}"
        );
    }
}
