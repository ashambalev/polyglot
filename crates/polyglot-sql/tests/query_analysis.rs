use polyglot_sql::{
    analyze_query, AnalyzeQueryOptions, DialectType, QueryShape, ReferenceConfidence,
    TransformKind, ValidationSchema,
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
                    {"name": "total", "type": "FLOAT"}
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
