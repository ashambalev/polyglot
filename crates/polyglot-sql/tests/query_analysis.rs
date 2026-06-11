use polyglot_sql::{
    analyze_query, scope::SourceKind, AnalyzeQueryOptions, DialectType, QueryShape,
    ReferenceConfidence, TransformKind, ValidationSchema,
};
use serde_json::json;

fn schema() -> ValidationSchema {
    serde_json::from_value(json!({
        "tables": [
            {
                "name": "users",
                "columns": [
                    {"name": "id", "type": "INT"},
                    {"name": "name", "type": "TEXT"}
                ]
            },
            {
                "name": "orders",
                "columns": [
                    {"name": "id", "type": "INT"},
                    {"name": "user_id", "type": "INT"},
                    {"name": "customer_id", "type": "INT"},
                    {"name": "amount", "type": "DECIMAL(10,2)"},
                    {"name": "total", "type": "FLOAT"}
                ]
            },
            {
                "name": "customers",
                "columns": [
                    {"name": "id", "type": "INT"},
                    {"name": "name", "type": "TEXT"}
                ]
            },
            {
                "name": "x",
                "columns": [{"name": "a", "type": "INT"}]
            },
            {
                "name": "y",
                "columns": [{"name": "b", "type": "INT"}]
            }
        ]
    }))
    .unwrap()
}

#[test]
fn analyze_query_reports_projection_relations_and_types() {
    let analysis = analyze_query(
        "SELECT u.id, CAST(o.total AS TEXT) AS total_text, 1 AS one \
         FROM users AS u JOIN orders AS o ON u.id = o.user_id",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(analysis.shape, QueryShape::Select);
    assert_eq!(analysis.projections.len(), 3);
    assert_eq!(analysis.projections[0].name.as_deref(), Some("id"));
    assert_eq!(
        analysis.projections[0].transform_kind,
        TransformKind::Direct
    );
    assert_eq!(analysis.projections[1].transform_kind, TransformKind::Cast);
    assert_eq!(analysis.projections[1].cast_type.as_deref(), Some("TEXT"));
    assert_eq!(
        analysis.projections[2].transform_kind,
        TransformKind::Constant
    );

    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.name == "users" && relation.alias.as_deref() == Some("u")));
    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.name == "orders" && relation.columns.contains(&"total".into())));
    assert!(analysis
        .base_tables
        .iter()
        .any(|relation| relation.name == "orders"));
    assert!(analysis
        .base_tables
        .iter()
        .any(|relation| relation.name == "users"));

    let total = &analysis.projections[1].upstream;
    assert!(total.iter().any(|reference| {
        reference.table.as_deref() == Some("orders")
            && reference.column == "total"
            && reference.confidence == ReferenceConfidence::Resolved
    }));
}

#[test]
fn analyze_query_follows_cte_lineage() {
    let analysis = analyze_query(
        "WITH base AS (SELECT id FROM users) SELECT id FROM base",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(analysis.ctes, vec!["base"]);
    assert!(analysis.projections[0].upstream.iter().any(|reference| {
        reference.table.as_deref() == Some("users") && reference.column == "id"
    }));
    assert_eq!(analysis.base_tables.len(), 1);
    assert_eq!(analysis.base_tables[0].name, "users");
}

#[test]
fn analyze_query_reports_set_operations() {
    let analysis = analyze_query(
        "SELECT a FROM x UNION ALL SELECT b FROM y",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(analysis.shape, QueryShape::SetOperation);
    assert_eq!(analysis.set_operations.len(), 1);
    assert_eq!(analysis.set_operations[0].kind, "union");
    assert!(analysis.set_operations[0].all);
    assert_eq!(analysis.set_operations[0].output_columns, vec!["a"]);
    assert_eq!(analysis.set_operations[0].branches.len(), 2);
    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.name == "x"));
    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.name == "y"));
    assert!(analysis
        .base_tables
        .iter()
        .any(|relation| relation.name == "x"));
    assert!(analysis
        .base_tables
        .iter()
        .any(|relation| relation.name == "y"));
    assert_eq!(
        analysis.set_operations[0].branches[0].projections[0]
            .name
            .as_deref(),
        Some("a")
    );
}

#[test]
fn analyze_query_rejects_non_query_statements() {
    let err = analyze_query(
        "CREATE TABLE t (a INT)",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: None,
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("requires a SELECT"));
}

#[test]
fn analyze_query_preserves_physical_table_aliases_in_lineage() {
    let analysis = analyze_query(
        "SELECT o.id FROM orders AS o",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    let reference = analysis.projections[0]
        .upstream
        .iter()
        .find(|reference| reference.column == "id")
        .unwrap();
    assert_eq!(reference.source_name.as_deref(), Some("orders"));
    assert_eq!(reference.source_alias.as_deref(), Some("o"));
    assert_eq!(reference.table.as_deref(), Some("orders"));
}

#[test]
fn analyze_query_limits_qualified_star_to_matching_source() {
    let analysis = analyze_query(
        "SELECT o.* FROM orders AS o JOIN customers AS c ON o.customer_id = c.id",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(analysis.projections.len(), 5);
    let mut projection_names: Vec<_> = analysis
        .projections
        .iter()
        .filter_map(|projection| projection.name.as_deref())
        .collect();
    projection_names.sort_unstable();
    assert_eq!(
        projection_names,
        vec!["amount", "customer_id", "id", "total", "user_id"]
    );
    assert!(analysis
        .projections
        .iter()
        .all(|projection| !projection.is_star));
    assert!(analysis.projections.iter().all(|projection| {
        projection
            .upstream
            .iter()
            .all(|reference| reference.table.as_deref() == Some("orders"))
    }));
}

#[test]
fn analyze_query_resolves_unique_unqualified_columns_with_alias_schema() {
    let analysis = analyze_query(
        "SELECT amount FROM orders AS o JOIN customers AS c ON o.customer_id = c.id",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    let reference = analysis.projections[0]
        .upstream
        .iter()
        .find(|reference| reference.column == "amount")
        .unwrap();
    assert_eq!(reference.table.as_deref(), Some("orders"));
    assert_eq!(reference.source_alias.as_deref(), Some("o"));
    assert_eq!(reference.confidence, ReferenceConfidence::Resolved);
}

#[test]
fn analyze_query_preserves_precise_schema_type_hints() {
    let analysis = analyze_query(
        "SELECT amount FROM orders",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(
        analysis.projections[0].type_hint.as_deref(),
        Some("DECIMAL(10, 2)")
    );
}

#[test]
fn analyze_query_expands_unqualified_star_with_schema() {
    let analysis = analyze_query(
        "SELECT * FROM orders",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    let mut names: Vec<_> = analysis
        .projections
        .iter()
        .filter_map(|projection| projection.name.as_deref())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec!["amount", "customer_id", "id", "total", "user_id"]
    );
    assert!(analysis
        .projections
        .iter()
        .all(|projection| !projection.is_star));
}

#[test]
fn analyze_query_classifies_typed_aggregates() {
    let analysis = analyze_query(
        "SELECT COUNT(*) AS rows, SUM(amount) AS amount_sum FROM orders",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    assert_eq!(
        analysis.projections[0].transform_kind,
        TransformKind::Aggregation
    );
    assert_eq!(
        analysis.projections[1].transform_kind,
        TransformKind::Aggregation
    );
}

#[test]
fn analyze_query_reports_transitive_base_tables() {
    let analysis = analyze_query(
        "WITH paid AS (SELECT customer_id FROM orders) \
         SELECT c.name FROM customers AS c \
         JOIN (SELECT customer_id FROM paid) AS p ON c.id = p.customer_id",
        AnalyzeQueryOptions {
            dialect: DialectType::Generic,
            schema: Some(schema()),
        },
    )
    .unwrap();

    let base_table_names: Vec<_> = analysis
        .base_tables
        .iter()
        .map(|relation| relation.name.as_str())
        .collect();
    assert_eq!(base_table_names, vec!["customers", "orders"]);
    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.name == "customers"));
    assert!(analysis
        .relations
        .iter()
        .any(|relation| relation.kind == SourceKind::DerivedTable));
}
