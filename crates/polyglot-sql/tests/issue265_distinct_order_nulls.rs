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
