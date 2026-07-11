//! Dialect Matrix Transpilation Tests
//!
//! Tests transpilation between all priority dialect pairs (7 dialects = 42 pairs).
//! Priority dialects: Generic, PostgreSQL, MySQL, BigQuery, Snowflake, DuckDB, TSQL
//!
//! Each test ensures SQL can be transpiled from one dialect to another
//! with expected function and syntax transformations.

use polyglot_sql::dialects::{Dialect, DialectType};
use polyglot_sql::{TranspileOptions, UnsupportedLevel};

/// Helper function to test transpilation between dialects
fn transpile(sql: &str, from: DialectType, to: DialectType) -> String {
    let source_dialect = Dialect::get(from);
    let result = source_dialect.transpile(sql, to).expect(&format!(
        "Failed to transpile: {} from {:?} to {:?}",
        sql, from, to
    ));
    result[0].clone()
}

/// Helper to verify transpilation produces valid SQL (doesn't crash)
fn transpile_succeeds(sql: &str, from: DialectType, to: DialectType) -> bool {
    let source_dialect = Dialect::get(from);
    source_dialect.transpile(sql, to).is_ok()
}

mod strict_unsupported_regressions {
    use super::*;

    fn transpile_with_level(
        sql: &str,
        read: DialectType,
        write: DialectType,
        level: UnsupportedLevel,
    ) -> polyglot_sql::Result<Vec<String>> {
        Dialect::get(read).transpile_with(
            sql,
            write,
            TranspileOptions::default().with_unsupported_level(level),
        )
    }

    #[test]
    fn default_transpile_still_allows_known_unsupported_leftovers() {
        let cases = [
            (
                "WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM t WHERE n < 3) SELECT * FROM t",
                DialectType::PostgreSQL,
                DialectType::Fabric,
            ),
            (
                "SELECT ARRAY_AGG(x) FROM t",
                DialectType::PostgreSQL,
                DialectType::Fabric,
            ),
            (
                "SELECT ROW_TO_JSON(t) FROM t",
                DialectType::PostgreSQL,
                DialectType::Fabric,
            ),
            (
                "SELECT lpad(s, 5, 'x') FROM t",
                DialectType::PostgreSQL,
                DialectType::Fabric,
            ),
            (
                "SELECT * FROM t, LATERAL (SELECT 1 AS x) AS s",
                DialectType::PostgreSQL,
                DialectType::Fabric,
            ),
        ];

        for (sql, read, write) in cases {
            let result = Dialect::get(read).transpile(sql, write);
            assert!(
                result.is_ok(),
                "default transpile should not reject {read:?} -> {write:?}: {result:?}"
            );
        }
    }

    #[test]
    fn strict_transpile_rejects_fabric_recursive_ctes() {
        let err = transpile_with_level(
            "WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM t WHERE n < 3) SELECT * FROM t",
            DialectType::PostgreSQL,
            DialectType::Fabric,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Fabric transpile should reject recursive CTEs");

        assert!(err.to_string().contains("recursive CTEs"));
    }

    #[test]
    fn strict_transpile_rejects_hive_recursive_ctes() {
        let err = transpile_with_level(
            "WITH RECURSIVE t(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM t WHERE n < 3) SELECT * FROM t",
            DialectType::PostgreSQL,
            DialectType::Hive,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Hive transpile should reject recursive CTEs");

        assert!(err.to_string().contains("recursive CTEs"));
    }

    #[test]
    fn strict_transpile_rejects_remaining_lateral_for_tsql_targets() {
        let err = transpile_with_level(
            "SELECT * FROM (orders o JOIN LATERAL (SELECT 1 AS id) a USING (id))",
            DialectType::PostgreSQL,
            DialectType::Fabric,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Fabric transpile should reject remaining LATERAL");

        assert!(err.to_string().contains("LATERAL"));
    }

    #[test]
    fn strict_transpile_rejects_except_intersect_all_for_tsql_targets() {
        let cases = [
            (
                "SELECT a FROM t EXCEPT ALL SELECT b FROM t",
                "EXCEPT ALL is not supported",
            ),
            (
                "SELECT a FROM t INTERSECT ALL SELECT b FROM t",
                "INTERSECT ALL is not supported",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                let err = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    target,
                    UnsupportedLevel::Raise,
                )
                .expect_err("strict transpile should reject unsupported set operation");

                assert!(
                    err.to_string().contains(expected),
                    "expected {expected:?} in error for {target:?}, got {err}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_allows_distinct_except_intersect_for_tsql_targets() {
        let cases = [
            "SELECT a FROM t EXCEPT SELECT b FROM t",
            "SELECT a FROM t INTERSECT SELECT b FROM t",
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for sql in cases {
                let result = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    target,
                    UnsupportedLevel::Raise,
                )
                .unwrap_or_else(|err| {
                    panic!("strict {target:?} transpile should allow distinct set operation: {err}")
                });

                assert_eq!(result.len(), 1);
                assert!(
                    !result[0].contains(" ALL "),
                    "distinct set operation should not contain ALL for {target:?}: {}",
                    result[0]
                );
            }
        }
    }

    #[test]
    fn strict_transpile_rejects_remaining_unnest_for_unsupported_targets() {
        let err = transpile_with_level(
            "SELECT UNNEST(arr) FROM t",
            DialectType::PostgreSQL,
            DialectType::Redshift,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Redshift transpile should reject remaining UNNEST");

        assert!(err.to_string().contains("UNNEST"));
    }

    #[test]
    fn strict_transpile_allows_rewritten_unnest_for_supported_targets() {
        let result = transpile_with_level(
            "SELECT * FROM t CROSS JOIN UNNEST(arr) AS x",
            DialectType::BigQuery,
            DialectType::DuckDB,
            UnsupportedLevel::Raise,
        )
        .expect("strict DuckDB transpile should allow supported/re-written UNNEST");

        assert_eq!(result.len(), 1);
    }

    #[test]
    fn strict_transpile_rejects_remaining_explode_for_unsupported_targets() {
        let err = transpile_with_level(
            "SELECT EXPLODE(arr) FROM t",
            DialectType::Generic,
            DialectType::SQLite,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict SQLite transpile should reject remaining EXPLODE");

        assert!(err.to_string().contains("EXPLODE"));
    }

    #[test]
    fn strict_transpile_rejects_array_agg_for_lossy_targets() {
        let err = transpile_with_level(
            "SELECT ARRAY_AGG(x) FROM t",
            DialectType::PostgreSQL,
            DialectType::Fabric,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Fabric transpile should reject ARRAY_AGG");

        assert!(err.to_string().contains("ARRAY_AGG"));
    }

    #[test]
    fn strict_transpile_rejects_regex_predicates_for_tsql_targets() {
        let sqls = [
            "SELECT 1 FROM t WHERE p_brand SIMILAR TO 'Brand#[1-3][0-9]'",
            "SELECT 1 FROM t WHERE c_phone ~ '^1[0-9]'",
            "SELECT 1 FROM t WHERE c_phone !~ '^1[0-9]'",
            "SELECT 1 FROM t WHERE c_phone ~* '^1[0-9]'",
            "SELECT 1 FROM t WHERE c_phone !~* '^1[0-9]'",
            "SELECT 1 FROM t WHERE REGEXP_LIKE(c_phone, '^1[0-9]')",
        ];

        for sql in sqls {
            for write in [DialectType::Fabric, DialectType::TSQL] {
                let err = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    write,
                    UnsupportedLevel::Raise,
                )
                .expect_err("strict TSQL/Fabric transpile should reject regex predicates");

                assert!(
                    err.to_string().contains("regular expression predicates"),
                    "unexpected error for PostgreSQL -> {write:?}: {err}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_rejects_residual_fetch_ties_overlaps_and_date_bin_for_tsql_targets() {
        let cases = [
            (
                "SELECT store_id FROM orders ORDER BY promised_at OFFSET 2 ROWS FETCH FIRST 5 ROWS WITH TIES",
                "FETCH WITH TIES without TOP",
            ),
            ("SELECT 1 FROM t WHERE a OVERLAPS b", "OVERLAPS"),
            (
                "SELECT date_bin('1 month', completed_at, TIMESTAMP '2001-01-01') FROM order_fulfillment_history",
                "DATE_BIN",
            ),
        ];

        for target in [DialectType::Fabric, DialectType::TSQL] {
            for (sql, expected) in cases {
                let err = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    target,
                    UnsupportedLevel::Raise,
                )
                .expect_err("strict TSQL/Fabric transpile should reject residual unsupported node");

                assert!(
                    err.to_string().contains(expected),
                    "expected {expected:?} for PostgreSQL -> {target:?}, got {err}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_rejects_postgres_only_functions() {
        let err = transpile_with_level(
            "SELECT ROW_TO_JSON(docs), TO_TSVECTOR(body) FROM docs",
            DialectType::PostgreSQL,
            DialectType::Fabric,
            UnsupportedLevel::Raise,
        )
        .expect_err("strict Fabric transpile should reject PostgreSQL-only functions");

        let message = err.to_string();
        assert!(message.contains("ROW_TO_JSON"));
        assert!(message.contains("TO_TSVECTOR"));
    }

    #[test]
    fn strict_transpile_rejects_postgres_only_scalar_functions_for_tsql_targets() {
        let cases = [
            ("SELECT lpad(s, 5, 'x') FROM t", "LPAD"),
            ("SELECT rpad(s, 5, 'x') FROM t", "RPAD"),
            ("SELECT split_part(s, ',', 1) FROM t", "SPLIT_PART"),
            ("SELECT initcap(s) FROM t", "INITCAP"),
            ("SELECT to_jsonb(s) FROM t", "TO_JSONB"),
        ];

        for (sql, function_name) in cases {
            for write in [DialectType::Fabric, DialectType::TSQL] {
                let err = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    write,
                    UnsupportedLevel::Raise,
                )
                .expect_err("strict TSQL/Fabric transpile should reject PostgreSQL-only functions");

                assert!(
                    err.to_string().contains(function_name),
                    "unexpected error for {function_name} to {write:?}: {err}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_rejects_postgres_json_functions_for_tsql_targets() {
        let cases = [
            ("SELECT to_json(s) FROM t", "TO_JSON"),
            ("SELECT row_to_json(t) FROM t", "ROW_TO_JSON"),
            ("SELECT jsonb_object_agg(k, s) FROM t", "JSONB_OBJECT_AGG"),
        ];

        for (sql, function_name) in cases {
            for write in [DialectType::Fabric, DialectType::TSQL] {
                let err = transpile_with_level(
                    sql,
                    DialectType::PostgreSQL,
                    write,
                    UnsupportedLevel::Raise,
                )
                .expect_err("strict TSQL/Fabric transpile should reject PostgreSQL JSON functions");

                assert!(
                    err.to_string().contains(function_name),
                    "unexpected error for {function_name} to {write:?}: {err}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_honors_max_unsupported() {
        let err = Dialect::get(DialectType::PostgreSQL)
            .transpile_with(
                "SELECT ARRAY_AGG(x), ROW_TO_JSON(docs), TO_TSVECTOR(body) FROM docs",
                DialectType::Fabric,
                TranspileOptions::strict().with_max_unsupported(2),
            )
            .expect_err("strict Fabric transpile should report unsupported diagnostics");

        let message = err.to_string();
        assert!(message.contains("ARRAY_AGG"));
        assert!(message.contains("ROW_TO_JSON"));
        assert!(message.contains("... and 1 more"));
    }

    #[test]
    fn transpile_options_deserialize_unsupported_level_from_json() {
        let opts: TranspileOptions =
            serde_json::from_str(r#"{"unsupportedLevel":"raise","maxUnsupported":2}"#)
                .expect("options JSON should deserialize");

        assert_eq!(opts.unsupported_level, UnsupportedLevel::Raise);
        assert_eq!(opts.max_unsupported, 2);
        assert!(!opts.pretty);
    }

    #[test]
    fn transpile_options_deserialize_complexity_guard_from_json() {
        let opts: TranspileOptions =
            serde_json::from_str(r#"{"complexityGuard":{"maxFunctionCallDepth":128}}"#)
                .expect("options JSON should deserialize");

        assert_eq!(opts.complexity_guard.max_function_call_depth, Some(128));
        assert_eq!(
            opts.complexity_guard.max_parenthesis_depth,
            polyglot_sql::ComplexityGuardOptions::default().max_parenthesis_depth
        );
    }
}

mod tsql_fabric_regressions {
    use super::*;
    use polyglot_sql::builder::{cast, col};

    fn pg_to_target(sql: &str, target: DialectType) -> String {
        Dialect::get(DialectType::PostgreSQL)
            .transpile_with(sql, target, TranspileOptions::strict())
            .unwrap_or_else(|err| panic!("transpile failed for {sql:?} to {target:?}: {err}"))
            .into_iter()
            .next()
            .expect("expected one generated statement")
    }

    fn pg_to_target_with_options(
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
    fn postgres_cte_materialization_hints_are_stripped_for_tsql_and_fabric() {
        let cases = [
            (
                "WITH x AS MATERIALIZED (SELECT f1 FROM t) SELECT * FROM x WHERE f1 = 1",
                "WITH x AS (SELECT f1 FROM t) SELECT * FROM x WHERE f1 = 1",
            ),
            (
                "WITH x AS NOT MATERIALIZED (SELECT f1 FROM t) SELECT * FROM x WHERE f1 = 1",
                "WITH x AS (SELECT f1 FROM t) SELECT * FROM x WHERE f1 = 1",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                let result = pg_to_target(sql, target);
                assert_eq!(result, expected, "failed for target {target:?}");
            }
        }
    }

    #[test]
    fn nested_ctes_in_from_subqueries_are_hoisted_for_tsql_and_fabric() {
        let sql = "WITH x AS (SELECT * FROM t) SELECT * FROM (WITH y AS (SELECT * FROM x) SELECT * FROM y) ss";
        let expected =
            "WITH x AS (SELECT * FROM t), y AS (SELECT * FROM x) SELECT * FROM (SELECT * FROM y) AS ss";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target(sql, target);
            assert_eq!(result, expected, "failed for target {target:?}");
        }
    }

    #[test]
    fn tsql_and_fabric_nvarchar_generation_differ_by_target() {
        let expr = cast(col("x"), "NVARCHAR(MAX)").into_inner();

        let tsql = Dialect::get(DialectType::TSQL).generate(&expr).unwrap();
        assert_eq!(tsql, "CAST(x AS NVARCHAR(MAX))");

        let fabric = Dialect::get(DialectType::Fabric).generate(&expr).unwrap();
        assert_eq!(fabric, "CAST(x AS VARCHAR(MAX))");
    }

    #[test]
    fn tsql_preserves_nvarchar_in_cast_parsing() {
        let result = Dialect::get(DialectType::TSQL)
            .transpile("SELECT CAST(x AS NVARCHAR(100))", DialectType::TSQL)
            .unwrap();

        assert_eq!(result[0], "SELECT CAST(x AS NVARCHAR(100))");
    }

    #[test]
    fn fabric_maps_nvarchar_to_supported_varchar() {
        let result = Dialect::get(DialectType::Fabric)
            .transpile("SELECT CAST(x AS NVARCHAR(MAX))", DialectType::Fabric)
            .unwrap();

        assert_eq!(result[0], "SELECT CAST(x AS VARCHAR(MAX))");
    }

    #[test]
    fn distinct_order_by_asc_nulls_last_uses_valid_wrapper_for_tsql_and_fabric() {
        let expected = "SELECT v FROM (SELECT DISTINCT v, CASE WHEN v IS NULL THEN 1 ELSE 0 END AS _polyglot_order_null_0, v AS _polyglot_order_key_0 FROM t) AS _polyglot_distinct_order ORDER BY _polyglot_order_null_0, _polyglot_order_key_0";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT DISTINCT v FROM t ORDER BY v", target),
                expected,
                "failed for target {target:?}"
            );
        }
    }

    #[test]
    fn distinct_order_by_desc_nulls_first_uses_valid_wrapper_for_tsql_and_fabric() {
        let expected = "SELECT v FROM (SELECT DISTINCT v, CASE WHEN v IS NULL THEN 1 ELSE 0 END AS _polyglot_order_null_0, v AS _polyglot_order_key_0 FROM t) AS _polyglot_distinct_order ORDER BY _polyglot_order_null_0 DESC, _polyglot_order_key_0 DESC";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT DISTINCT v FROM t ORDER BY v DESC", target),
                expected,
                "failed for target {target:?}"
            );
        }
    }

    #[test]
    fn distinct_order_by_alias_resolves_to_selected_expression() {
        let expected = "SELECT x FROM (SELECT DISTINCT v AS x, CASE WHEN v IS NULL THEN 1 ELSE 0 END AS _polyglot_order_null_0, v AS _polyglot_order_key_0 FROM t) AS _polyglot_distinct_order ORDER BY _polyglot_order_null_0, _polyglot_order_key_0";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT DISTINCT v AS x FROM t ORDER BY x", target),
                expected,
                "failed for target {target:?}"
            );
        }
    }

    #[test]
    fn distinct_order_by_target_default_null_ordering_does_not_wrap() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT DISTINCT v FROM t ORDER BY v NULLS FIRST", target),
                "SELECT DISTINCT v FROM t ORDER BY v",
                "failed for target {target:?}"
            );
        }
    }

    #[test]
    fn non_distinct_order_by_keeps_existing_tsql_null_ordering_emulation() {
        let expected = "SELECT v FROM t ORDER BY CASE WHEN v IS NULL THEN 1 ELSE 0 END, v";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT v FROM t ORDER BY v", target),
                expected,
                "failed for target {target:?}"
            );
        }
    }

    #[test]
    fn strict_mode_rejects_distinct_order_by_unselected_expression() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            let err = Dialect::get(DialectType::PostgreSQL)
                .transpile_with(
                    "SELECT DISTINCT v FROM t ORDER BY LOWER(v)",
                    target,
                    TranspileOptions::strict(),
                )
                .expect_err("strict mode should reject unselected emulated ORDER BY expression");

            assert!(
                err.to_string()
                    .contains("SELECT DISTINCT with emulated NULL ordering"),
                "unexpected error for {target:?}: {err}"
            );
        }
    }

    #[test]
    fn nested_distinct_on_rewrites_for_tsql_and_fabric() {
        let sql = "SELECT * FROM foo WHERE id IN (SELECT id2 FROM (SELECT DISTINCT ON (id2) id1, id2 FROM bar) AS s)";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target(sql, target);
            assert!(
                !result.contains("DISTINCT ON"),
                "nested DISTINCT ON should be eliminated for {target:?}: {result}"
            );
            assert!(
                result.contains("ROW_NUMBER() OVER (PARTITION BY id2 ORDER BY id2)"),
                "nested DISTINCT ON should use ROW_NUMBER for {target:?}: {result}"
            );
        }
    }

    #[test]
    fn cte_distinct_on_rewrites_for_tsql_and_fabric() {
        let sql = "WITH s AS (SELECT DISTINCT ON (id2) id1, id2 FROM bar) SELECT id2 FROM s";

        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target(sql, target);
            assert!(
                !result.contains("DISTINCT ON"),
                "CTE DISTINCT ON should be eliminated for {target:?}: {result}"
            );
            assert!(
                result.contains("ROW_NUMBER() OVER (PARTITION BY id2 ORDER BY id2)"),
                "CTE DISTINCT ON should use ROW_NUMBER for {target:?}: {result}"
            );
        }
    }

    #[test]
    fn default_transpile_resolves_positional_order_by_for_tsql_and_fabric() {
        let cases = [
            (
                "SELECT f1 FROM t ORDER BY 1",
                "SELECT f1 FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1",
            ),
            (
                "SELECT f1 FROM t ORDER BY 1 DESC",
                "SELECT f1 FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END DESC, f1 DESC",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                let result = pg_to_target_with_options(sql, target, TranspileOptions::default())
                    .unwrap_or_else(|err| panic!("default {target:?} transpile failed: {err}"));
                assert_eq!(result, expected, "failed for {target:?}: {sql}");
            }
        }
    }

    #[test]
    fn default_transpile_resolves_positional_order_by_on_set_operations_for_tsql_and_fabric() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target_with_options(
                "SELECT f1 FROM a UNION SELECT f1 FROM b ORDER BY 1",
                target,
                TranspileOptions::default(),
            )
            .unwrap_or_else(|err| panic!("default {target:?} transpile failed: {err}"));

            assert!(
                result.ends_with("ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1"),
                "set operation should resolve positional ORDER BY for {target:?}: {result}"
            );
            assert!(
                !result.contains("CASE WHEN 1 IS NULL"),
                "set operation should not emit a constant NULL-ordering CASE for {target:?}: {result}"
            );
        }
    }

    #[test]
    fn strict_transpile_resolves_positional_order_by_null_ordering_for_tsql_and_fabric() {
        let cases = [
            (
                "SELECT f1 FROM t ORDER BY 1",
                "SELECT f1 FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1",
            ),
            (
                "SELECT f1 FROM t ORDER BY 1 DESC",
                "SELECT f1 FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END DESC, f1 DESC",
            ),
            (
                "SELECT f1, f2 FROM t ORDER BY 2",
                "SELECT f1, f2 FROM t ORDER BY CASE WHEN f2 IS NULL THEN 1 ELSE 0 END, f2",
            ),
            (
                "SELECT f1 AS x FROM t ORDER BY 1",
                "SELECT f1 AS x FROM t ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1",
            ),
            (
                "SELECT f1 + 1 AS x FROM t ORDER BY 1",
                "SELECT f1 + 1 AS x FROM t ORDER BY CASE WHEN f1 + 1 IS NULL THEN 1 ELSE 0 END, f1 + 1",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                let result = pg_to_target_with_options(sql, target, TranspileOptions::strict())
                    .unwrap_or_else(|err| panic!("strict {target:?} transpile failed: {err}"));
                assert_eq!(result, expected, "failed for {target:?}: {sql}");
            }
        }
    }

    #[test]
    fn strict_transpile_resolves_positional_order_by_on_set_operations_for_tsql_and_fabric() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target_with_options(
                "SELECT f1 FROM a UNION SELECT f1 FROM b ORDER BY 1",
                target,
                TranspileOptions::strict(),
            )
            .unwrap_or_else(|err| panic!("strict {target:?} transpile failed: {err}"));

            assert!(
                result.ends_with("ORDER BY CASE WHEN f1 IS NULL THEN 1 ELSE 0 END, f1"),
                "set operation should resolve positional ORDER BY for {target:?}: {result}"
            );
            assert!(
                !result.contains("CASE WHEN 1 IS NULL"),
                "set operation should not emit a constant NULL-ordering CASE for {target:?}: {result}"
            );
        }
    }

    #[test]
    fn strict_transpile_rejects_unresolved_positional_order_by_null_ordering_for_tsql_and_fabric() {
        let cases = [
            "SELECT * FROM t ORDER BY 1",
            "SELECT 1 FROM t ORDER BY 1",
            "SELECT f1 + 1 FROM a UNION SELECT f1 + 1 FROM b ORDER BY 1",
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for sql in cases {
                let err = pg_to_target_with_options(sql, target, TranspileOptions::strict())
                    .expect_err("strict transpile should reject unresolved positional ordering");
                let message = err.to_string();
                assert!(
                    message.contains("positional ordering"),
                    "unexpected error for {target:?} {sql:?}: {message}"
                );
            }
        }
    }

    #[test]
    fn strict_transpile_keeps_named_order_by_null_ordering_rewrite_for_tsql_and_fabric() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            let result = pg_to_target_with_options(
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

    #[test]
    fn null_safe_comparisons_in_select_list_are_materialized_as_bit_values() {
        let cases = [
            (
                "SELECT f1 IS DISTINCT FROM 2 AS not2 FROM t",
                "SELECT CAST(CASE WHEN f1 IS DISTINCT FROM 2 THEN 1 ELSE 0 END AS BIT) AS not2 FROM t",
            ),
            (
                "SELECT f1 IS NOT DISTINCT FROM 2 AS is2 FROM t",
                "SELECT CAST(CASE WHEN f1 IS NOT DISTINCT FROM 2 THEN 1 ELSE 0 END AS BIT) AS is2 FROM t",
            ),
            (
                "SELECT (f1 IS DISTINCT FROM f2) AS diff FROM t",
                "SELECT CAST(CASE WHEN (f1 IS DISTINCT FROM f2) THEN 1 ELSE 0 END AS BIT) AS diff FROM t",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
            }
        }
    }

    #[test]
    fn null_safe_comparisons_in_scalar_sort_contexts_are_materialized() {
        for target in [DialectType::TSQL, DialectType::Fabric] {
            assert_eq!(
                pg_to_target("SELECT f1 FROM t ORDER BY f1 IS DISTINCT FROM 2", target),
                "SELECT f1 FROM t ORDER BY CASE WHEN CAST(CASE WHEN f1 IS DISTINCT FROM 2 THEN 1 ELSE 0 END AS BIT) IS NULL THEN 1 ELSE 0 END, CAST(CASE WHEN f1 IS DISTINCT FROM 2 THEN 1 ELSE 0 END AS BIT)",
                "failed for {target:?}"
            );
        }
    }

    #[test]
    fn null_safe_comparisons_in_predicate_contexts_remain_predicates() {
        let cases = [
            (
                "SELECT f1 FROM t WHERE f1 IS DISTINCT FROM 2",
                "SELECT f1 FROM t WHERE f1 IS DISTINCT FROM 2",
            ),
            (
                "SELECT f1 FROM t WHERE f1 IS NOT DISTINCT FROM 2",
                "SELECT f1 FROM t WHERE f1 IS NOT DISTINCT FROM 2",
            ),
            (
                "SELECT f1 FROM t JOIN u ON t.f1 IS DISTINCT FROM u.f1",
                "SELECT f1 FROM t JOIN u ON t.f1 IS DISTINCT FROM u.f1",
            ),
        ];

        for target in [DialectType::TSQL, DialectType::Fabric] {
            for (sql, expected) in cases {
                assert_eq!(pg_to_target(sql, target), expected, "failed for {target:?}");
            }
        }
    }
}

mod fabric_regressions {
    use super::*;

    #[test]
    fn test_postgres_to_fabric_tpch_syntax() {
        assert_eq!(
            transpile(
                "SELECT DATE '1998-12-01'",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT CAST('1998-12-01' AS DATE)"
        );
        assert_eq!(
            transpile(
                "SELECT SUBSTRING(c_phone FROM 1 FOR 2)",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT SUBSTRING(c_phone, 1, 2)"
        );
        assert_eq!(
            transpile(
                "SELECT * FROM lineitem LIMIT 10",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT TOP 10 * FROM lineitem"
        );
        assert_eq!(
            transpile(
                "SELECT * FROM lineitem ORDER BY shipdate NULLS FIRST",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT * FROM lineitem ORDER BY shipdate"
        );
    }

    #[test]
    fn test_postgres_to_fabric_interval_arithmetic() {
        assert_eq!(
            transpile(
                "SELECT DATE '1998-12-01' + INTERVAL '90' DAY",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT DATEADD(DAY, 90, CAST('1998-12-01' AS DATE))"
        );
        assert_eq!(
            transpile(
                "SELECT shipdate - INTERVAL '3' DAY FROM lineitem",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT DATEADD(DAY, -3, shipdate) FROM lineitem"
        );
        assert_eq!(
            transpile(
                "SELECT shipdate - INTERVAL '-3' DAY FROM lineitem",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT DATEADD(DAY, 3, shipdate) FROM lineitem"
        );
        assert_eq!(
            transpile(
                "SELECT shipdate + INTERVAL '1 day' FROM lineitem",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT DATEADD(DAY, 1, shipdate) FROM lineitem"
        );
        assert_eq!(
            transpile(
                "SELECT shipdate + INTERVAL n DAY FROM lineitem",
                DialectType::PostgreSQL,
                DialectType::Fabric
            ),
            "SELECT DATEADD(DAY, n, shipdate) FROM lineitem"
        );
    }
}

// ============================================================================
// Basic SELECT Transpilation Tests
// ============================================================================

mod basic_select {
    use super::*;

    #[test]
    fn test_generic_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::DuckDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_postgres_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::DuckDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::PostgreSQL,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_mysql_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::DuckDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::MySQL,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_bigquery_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::DuckDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::BigQuery,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_snowflake_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::DuckDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Snowflake,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_duckdb_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::DuckDB,
            DialectType::TSQL
        ));
    }

    #[test]
    fn test_tsql_to_all() {
        let sql = "SELECT a, b FROM users WHERE id = 1";

        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::Generic
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::PostgreSQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::MySQL
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::BigQuery
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::Snowflake
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::TSQL,
            DialectType::DuckDB
        ));
    }
}

// ============================================================================
// NULL Handling Transpilation Tests (NVL, IFNULL, COALESCE)
// ============================================================================

mod null_handling {
    use super::*;

    // COALESCE should be preserved or converted appropriately
    #[test]
    fn test_coalesce_generic_to_postgres() {
        let result = transpile(
            "SELECT COALESCE(a, b)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.contains("COALESCE"),
            "PostgreSQL should use COALESCE: got {}",
            result
        );
    }

    #[test]
    fn test_coalesce_generic_to_mysql() {
        let result = transpile(
            "SELECT COALESCE(a, b)",
            DialectType::Generic,
            DialectType::MySQL,
        );
        // MySQL supports both COALESCE and IFNULL
        assert!(
            result.contains("COALESCE") || result.contains("IFNULL"),
            "MySQL should use COALESCE or IFNULL: got {}",
            result
        );
    }

    #[test]
    fn test_coalesce_generic_to_tsql() {
        let result = transpile(
            "SELECT COALESCE(a, b)",
            DialectType::Generic,
            DialectType::TSQL,
        );
        // SQL Server should convert 2-arg COALESCE to ISNULL
        assert!(
            result.contains("ISNULL") || result.contains("COALESCE"),
            "TSQL should use ISNULL or COALESCE: got {}",
            result
        );
    }

    // NVL transformations
    #[test]
    fn test_nvl_to_postgres() {
        let result = transpile(
            "SELECT NVL(a, b)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.contains("COALESCE"),
            "PostgreSQL should convert NVL to COALESCE: got {}",
            result
        );
    }

    #[test]
    fn test_nvl_to_mysql() {
        let result = transpile("SELECT NVL(a, b)", DialectType::Generic, DialectType::MySQL);
        assert!(
            result.contains("IFNULL") || result.contains("COALESCE"),
            "MySQL should convert NVL to IFNULL or COALESCE: got {}",
            result
        );
    }

    #[test]
    fn test_nvl_to_tsql() {
        let result = transpile("SELECT NVL(a, b)", DialectType::Generic, DialectType::TSQL);
        assert!(
            result.contains("ISNULL"),
            "TSQL should convert NVL to ISNULL: got {}",
            result
        );
    }

    // IFNULL transformations
    #[test]
    fn test_ifnull_to_postgres() {
        let result = transpile(
            "SELECT IFNULL(a, b)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.contains("COALESCE"),
            "PostgreSQL should convert IFNULL to COALESCE: got {}",
            result
        );
    }

    #[test]
    fn test_ifnull_to_snowflake() {
        let result = transpile(
            "SELECT IFNULL(a, b)",
            DialectType::Generic,
            DialectType::Snowflake,
        );
        // Snowflake supports both
        assert!(
            result.contains("IFNULL") || result.contains("COALESCE"),
            "Snowflake should accept IFNULL or COALESCE: got {}",
            result
        );
    }
}

// ============================================================================
// String Functions Transpilation Tests
// ============================================================================

mod string_functions {
    use super::*;

    // LENGTH vs LEN
    #[test]
    fn test_length_generic_to_postgres() {
        let result = transpile(
            "SELECT LENGTH(name)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("LENGTH"),
            "PostgreSQL uses LENGTH: got {}",
            result
        );
    }

    #[test]
    fn test_length_generic_to_tsql() {
        let result = transpile(
            "SELECT LENGTH(name)",
            DialectType::Generic,
            DialectType::TSQL,
        );
        assert!(
            result.to_uppercase().contains("LEN"),
            "TSQL should convert LENGTH to LEN: got {}",
            result
        );
    }

    // SUBSTR vs SUBSTRING
    #[test]
    fn test_substr_to_postgres() {
        let result = transpile(
            "SELECT SUBSTR(name, 1, 5)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("SUBSTRING") || result.to_uppercase().contains("SUBSTR"),
            "PostgreSQL uses SUBSTRING: got {}",
            result
        );
    }

    // CONCAT transformations
    #[test]
    fn test_concat_generic_to_postgres() {
        // Generic should support CONCAT function
        let result = transpile(
            "SELECT CONCAT(a, b)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("CONCAT") || result.contains("||"),
            "PostgreSQL should use CONCAT or ||: got {}",
            result
        );
    }

    #[test]
    fn test_postgres_dpipe_to_mysql_concat_issue_43() {
        let result = transpile(
            "SELECT 'A' || 'B'",
            DialectType::PostgreSQL,
            DialectType::MySQL,
        );
        assert_eq!(
            result, "SELECT CONCAT('A', 'B')",
            "PostgreSQL || should transpile to MySQL CONCAT: got {}",
            result
        );
    }

    #[test]
    fn test_mysql_dpipe_identity_is_or_issue_43() {
        let result = transpile("SELECT 'A' || 'B'", DialectType::MySQL, DialectType::MySQL);
        assert_eq!(
            result, "SELECT 'A' OR 'B'",
            "MySQL identity should treat || as OR: got {}",
            result
        );
    }

    #[test]
    fn test_generate_mysql_from_postgres_concat_ast_issue_43() {
        let ast = polyglot_sql::parse("SELECT 'A' || 'B'", DialectType::PostgreSQL).expect("parse");
        let mysql = Dialect::get(DialectType::MySQL);
        let sql = mysql.generate(&ast[0]).expect("generate");

        assert_eq!(
            sql, "SELECT CONCAT('A', 'B')",
            "MySQL generate should render semantic concat as CONCAT: got {}",
            sql
        );
    }

    // UPPER/LOWER should be universal
    #[test]
    fn test_upper_lower_preserved() {
        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let upper_result = transpile("SELECT UPPER(name)", DialectType::Generic, dialect);
            let lower_result = transpile("SELECT LOWER(name)", DialectType::Generic, dialect);

            assert!(
                upper_result.to_uppercase().contains("UPPER"),
                "{:?} should preserve UPPER: got {}",
                dialect,
                upper_result
            );
            assert!(
                lower_result.to_uppercase().contains("LOWER"),
                "{:?} should preserve LOWER: got {}",
                dialect,
                lower_result
            );
        }
    }
}

// ============================================================================
// Date/Time Functions Transpilation Tests
// ============================================================================

mod date_functions {
    use super::*;

    // NOW() transformations
    #[test]
    fn test_now_to_postgres() {
        let result = transpile(
            "SELECT NOW()",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("NOW")
                || result.to_uppercase().contains("CURRENT_TIMESTAMP"),
            "PostgreSQL should use NOW or CURRENT_TIMESTAMP: got {}",
            result
        );
    }

    #[test]
    fn test_now_to_tsql() {
        let result = transpile("SELECT NOW()", DialectType::Generic, DialectType::TSQL);
        assert!(
            result.to_uppercase().contains("GETDATE")
                || result.to_uppercase().contains("CURRENT_TIMESTAMP"),
            "TSQL should convert NOW to GETDATE: got {}",
            result
        );
    }

    // CURRENT_DATE should be supported or converted
    #[test]
    fn test_current_date_to_all() {
        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::Snowflake,
            DialectType::DuckDB,
        ];

        for dialect in dialects {
            let result = transpile("SELECT CURRENT_DATE", DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("CURRENT_DATE")
                    || result.to_uppercase().contains("GETDATE"),
                "{:?} should handle CURRENT_DATE: got {}",
                dialect,
                result
            );
        }
    }
}

// ============================================================================
// JSON Functions Transpilation Tests
// ============================================================================

mod json_functions {
    use super::*;

    #[test]
    fn test_json_search_mysql_to_duckdb_issue_42() {
        let sql = "SELECT JSON_SEARCH(meta, 'one', 'admin', NULL, '$.tags') IS NOT NULL FROM users";
        let result = transpile(sql, DialectType::MySQL, DialectType::DuckDB);
        let upper = result.to_uppercase();

        assert!(
            !upper.contains("JSON_SEARCH("),
            "DuckDB transpilation should rewrite JSON_SEARCH: got {}",
            result
        );
        assert!(
            upper.contains("JSON_TREE("),
            "DuckDB transpilation should use JSON_TREE lookup: got {}",
            result
        );
    }

    #[test]
    fn test_json_search_mysql_identity_preserved() {
        let sql = "SELECT JSON_SEARCH(meta, 'one', 'admin', NULL, '$.tags') FROM users";
        let result = transpile(sql, DialectType::MySQL, DialectType::MySQL);

        assert!(
            result.to_uppercase().contains("JSON_SEARCH("),
            "MySQL identity transpilation should preserve JSON_SEARCH: got {}",
            result
        );
    }
}

// ============================================================================
// Aggregate Functions Transpilation Tests
// ============================================================================

mod aggregate_functions {
    use super::*;

    // Basic aggregates should be universal
    #[test]
    fn test_count_preserved() {
        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile("SELECT COUNT(*) FROM t", DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("COUNT"),
                "{:?} should preserve COUNT: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_sum_avg_min_max() {
        let functions = ["SUM", "AVG", "MIN", "MAX"];
        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
        ];

        for func in functions {
            for dialect in dialects {
                let sql = format!("SELECT {}(x) FROM t", func);
                let result = transpile(&sql, DialectType::Generic, dialect);
                assert!(
                    result.to_uppercase().contains(func),
                    "{:?} should preserve {}: got {}",
                    dialect,
                    func,
                    result
                );
            }
        }
    }

    // GROUP_CONCAT / STRING_AGG / LISTAGG
    #[test]
    fn test_group_concat_to_postgres() {
        let result = transpile(
            "SELECT GROUP_CONCAT(name)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("STRING_AGG"),
            "PostgreSQL should convert GROUP_CONCAT to STRING_AGG: got {}",
            result
        );
    }

    #[test]
    fn test_group_concat_to_snowflake() {
        let result = transpile(
            "SELECT GROUP_CONCAT(name)",
            DialectType::Generic,
            DialectType::Snowflake,
        );
        assert!(
            result.to_uppercase().contains("LISTAGG"),
            "Snowflake should convert GROUP_CONCAT to LISTAGG: got {}",
            result
        );
    }

    #[test]
    fn test_group_concat_to_tsql() {
        let result = transpile(
            "SELECT GROUP_CONCAT(name)",
            DialectType::Generic,
            DialectType::TSQL,
        );
        assert!(
            result.to_uppercase().contains("STRING_AGG"),
            "TSQL should convert GROUP_CONCAT to STRING_AGG: got {}",
            result
        );
    }
}

// ============================================================================
// Statistical Functions Transpilation Tests
// ============================================================================

mod statistical_functions {
    use super::*;

    #[test]
    fn test_stddev_to_postgres() {
        let result = transpile(
            "SELECT STDDEV(x)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("STDDEV"),
            "PostgreSQL should preserve STDDEV: got {}",
            result
        );
    }

    #[test]
    fn test_stddev_to_tsql() {
        let result = transpile("SELECT STDDEV(x)", DialectType::Generic, DialectType::TSQL);
        assert!(
            result.to_uppercase().contains("STDEV"),
            "TSQL should convert STDDEV to STDEV: got {}",
            result
        );
    }

    #[test]
    fn test_variance_preserved() {
        let result = transpile(
            "SELECT VARIANCE(x)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("VARIANCE") || result.to_uppercase().contains("VAR"),
            "PostgreSQL should preserve VARIANCE: got {}",
            result
        );
    }
}

// ============================================================================
// Math Functions Transpilation Tests
// ============================================================================

mod math_functions {
    use super::*;

    #[test]
    fn test_random_to_postgres() {
        let result = transpile(
            "SELECT RANDOM()",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("RANDOM"),
            "PostgreSQL should use RANDOM: got {}",
            result
        );
    }

    #[test]
    fn test_rand_to_postgres() {
        let result = transpile(
            "SELECT RAND()",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("RANDOM") || result.to_uppercase().contains("RAND"),
            "PostgreSQL should convert RAND to RANDOM: got {}",
            result
        );
    }

    #[test]
    fn test_random_to_mysql() {
        let result = transpile("SELECT RANDOM()", DialectType::Generic, DialectType::MySQL);
        assert!(
            result.to_uppercase().contains("RAND"),
            "MySQL should convert RANDOM to RAND: got {}",
            result
        );
    }

    #[test]
    fn test_ln_to_postgres() {
        let result = transpile(
            "SELECT LN(x)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        assert!(
            result.to_uppercase().contains("LN"),
            "PostgreSQL should preserve LN: got {}",
            result
        );
    }

    #[test]
    fn test_ln_to_tsql() {
        let result = transpile("SELECT LN(x)", DialectType::Generic, DialectType::TSQL);
        assert!(
            result.to_uppercase().contains("LOG"),
            "TSQL should convert LN to LOG: got {}",
            result
        );
    }

    // CEIL/CEILING
    #[test]
    fn test_ceil_ceiling() {
        let result_pg = transpile(
            "SELECT CEIL(x)",
            DialectType::Generic,
            DialectType::PostgreSQL,
        );
        let result_tsql = transpile("SELECT CEIL(x)", DialectType::Generic, DialectType::TSQL);

        assert!(
            result_pg.to_uppercase().contains("CEIL"),
            "PostgreSQL should use CEIL: got {}",
            result_pg
        );
        assert!(
            result_tsql.to_uppercase().contains("CEILING")
                || result_tsql.to_uppercase().contains("CEIL"),
            "TSQL should use CEILING: got {}",
            result_tsql
        );
    }
}

// ============================================================================
// Cast Transpilation Tests
// ============================================================================

mod cast_functions {
    use super::*;

    #[test]
    fn test_cast_preserved() {
        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile("SELECT CAST(x AS INT)", DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("CAST"),
                "{:?} should preserve CAST: got {}",
                dialect,
                result
            );
        }
    }
}

// ============================================================================
// Complex Query Transpilation Tests
// ============================================================================

mod complex_queries {
    use super::*;

    #[test]
    fn test_join_query() {
        let sql = "SELECT u.name, o.total FROM users u JOIN orders o ON u.id = o.user_id";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            assert!(
                transpile_succeeds(sql, DialectType::Generic, dialect),
                "{:?} should handle JOIN query",
                dialect
            );
        }
    }

    #[test]
    fn test_in_subquery() {
        let sql = "SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            assert!(
                transpile_succeeds(sql, DialectType::Generic, dialect),
                "{:?} should handle IN subquery",
                dialect
            );
        }
    }

    #[test]
    fn test_from_subquery() {
        let sql = "SELECT * FROM (SELECT a, b FROM t) sub";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            assert!(
                transpile_succeeds(sql, DialectType::Generic, dialect),
                "{:?} should handle FROM subquery",
                dialect
            );
        }
    }

    #[test]
    fn test_group_by_having() {
        let sql =
            "SELECT category, COUNT(*) as cnt FROM products GROUP BY category HAVING COUNT(*) > 5";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            assert!(
                transpile_succeeds(sql, DialectType::Generic, dialect),
                "{:?} should handle GROUP BY HAVING",
                dialect
            );
        }
    }

    #[test]
    fn test_order_by_limit() {
        let sql = "SELECT * FROM users ORDER BY created_at DESC LIMIT 10";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("ORDER BY"),
                "{:?} should preserve ORDER BY: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_union_query() {
        let sql = "SELECT a FROM t1 UNION SELECT b FROM t2";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("UNION"),
                "{:?} should preserve UNION: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_case_expression() {
        let sql =
            "SELECT CASE WHEN x > 0 THEN 'positive' WHEN x < 0 THEN 'negative' ELSE 'zero' END";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("CASE") && result.to_uppercase().contains("WHEN"),
                "{:?} should preserve CASE WHEN: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_window_function() {
        let sql =
            "SELECT ROW_NUMBER() OVER (PARTITION BY dept ORDER BY salary DESC) FROM employees";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("ROW_NUMBER")
                    && result.to_uppercase().contains("OVER"),
                "{:?} should preserve window function: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_cte_query() {
        let sql = "WITH cte AS (SELECT id FROM users) SELECT * FROM cte";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("WITH"),
                "{:?} should preserve CTE: got {}",
                dialect,
                result
            );
        }
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn test_same_dialect_noop() {
        let sql = "SELECT a FROM users";

        let dialects = [
            DialectType::Generic,
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, dialect.clone(), dialect.clone());
            assert!(
                result.to_uppercase().contains("SELECT"),
                "{:?} to {:?} should preserve SELECT: got {}",
                dialect,
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_empty_query_list() {
        // Comment-only input should be handled gracefully
        let sql = "-- just a comment";
        let dialects = [DialectType::PostgreSQL, DialectType::MySQL];

        for dialect in dialects {
            let source = Dialect::get(DialectType::Generic);
            let result = source.transpile(sql, dialect);
            // Should either succeed with empty result or error gracefully
            match result {
                Ok(statements) => {
                    // Empty is acceptable
                    assert!(statements.is_empty() || !statements[0].is_empty());
                }
                Err(_) => {
                    // Error is also acceptable for comment-only input
                }
            }
        }
    }

    #[test]
    fn test_unicode_preservation() {
        let sql = "SELECT '日本語', '你好'";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.contains("日本語") && result.contains("你好"),
                "{:?} should preserve Unicode: got {}",
                dialect,
                result
            );
        }
    }

    #[test]
    fn test_nested_functions() {
        let sql = "SELECT UPPER(LOWER(TRIM(name)))";

        let dialects = [
            DialectType::PostgreSQL,
            DialectType::MySQL,
            DialectType::BigQuery,
            DialectType::Snowflake,
            DialectType::DuckDB,
            DialectType::TSQL,
        ];

        for dialect in dialects {
            let result = transpile(sql, DialectType::Generic, dialect);
            assert!(
                result.to_uppercase().contains("UPPER")
                    && result.to_uppercase().contains("LOWER")
                    && result.to_uppercase().contains("TRIM"),
                "{:?} should preserve nested functions: got {}",
                dialect,
                result
            );
        }
    }
}

// ============================================================================
// Secondary Dialects Matrix Tests
// ============================================================================

mod secondary_dialects {
    use super::*;

    #[test]
    fn test_oracle_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Oracle
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Oracle,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_sqlite_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::SQLite
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::SQLite,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_hive_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Hive
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Hive,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_spark_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Spark
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Spark,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_trino_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Trino
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Trino,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_redshift_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Redshift
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Redshift,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_clickhouse_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::ClickHouse
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::ClickHouse,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_databricks_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Databricks
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Databricks,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_presto_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::Presto
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::Presto,
            DialectType::Generic
        ));
    }

    #[test]
    fn test_cockroachdb_transpile() {
        let sql = "SELECT a, b FROM users WHERE id = 1";
        assert!(transpile_succeeds(
            sql,
            DialectType::Generic,
            DialectType::CockroachDB
        ));
        assert!(transpile_succeeds(
            sql,
            DialectType::CockroachDB,
            DialectType::Generic
        ));
    }
}
