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
fn bare_boolean_predicates_are_coerced_for_tsql_and_fabric() {
    let cases = [
        ("SELECT o FROM t WHERE b", "SELECT o FROM t WHERE b <> 0"),
        (
            "SELECT o FROM t WHERE NOT b",
            "SELECT o FROM t WHERE NOT b <> 0",
        ),
        (
            "SELECT o FROM t WHERE b AND c",
            "SELECT o FROM t WHERE b <> 0 AND c <> 0",
        ),
        (
            "SELECT o FROM t WHERE b OR c",
            "SELECT o FROM t WHERE b <> 0 OR c <> 0",
        ),
        (
            "SELECT o FROM t GROUP BY o HAVING b",
            "SELECT o FROM t GROUP BY o HAVING b <> 0",
        ),
        (
            "SELECT CASE WHEN b THEN 1 ELSE 0 END FROM t",
            "SELECT CASE WHEN b <> 0 THEN 1 ELSE 0 END FROM t",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}

#[test]
fn bare_boolean_join_on_predicates_are_coerced_for_tsql_and_fabric() {
    let cases = [
        (
            "SELECT o FROM t JOIN u ON t.b",
            "SELECT o FROM t JOIN u ON t.b <> 0",
        ),
        (
            "SELECT o FROM t JOIN u ON NOT t.b",
            "SELECT o FROM t JOIN u ON NOT t.b <> 0",
        ),
        (
            "SELECT o FROM t JOIN u ON t.b AND u.c",
            "SELECT o FROM t JOIN u ON t.b <> 0 AND u.c <> 0",
        ),
    ];

    for target in [DialectType::TSQL, DialectType::Fabric] {
        for (sql, expected) in cases {
            assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
        }
    }
}

#[test]
fn nested_boolean_predicates_are_coerced_for_tsql_and_fabric() {
    assert_eq!(
        pg_to_target(
            "SELECT * FROM (SELECT o FROM t WHERE b) AS x",
            DialectType::TSQL,
        ),
        "SELECT * FROM (SELECT o AS o FROM t WHERE b <> 0) AS x"
    );
    assert_eq!(
        pg_to_target(
            "SELECT * FROM (SELECT o FROM t WHERE b) AS x",
            DialectType::Fabric,
        ),
        "SELECT * FROM (SELECT o FROM t WHERE b <> 0) AS x"
    );

    for target in [DialectType::TSQL, DialectType::Fabric] {
        assert_eq!(
            pg_to_target(
                "WITH x AS (SELECT o FROM t WHERE b) SELECT o FROM x",
                target,
            ),
            "WITH x AS (SELECT o FROM t WHERE b <> 0) SELECT o FROM x",
            "failed for {target:?}"
        );
    }
}
