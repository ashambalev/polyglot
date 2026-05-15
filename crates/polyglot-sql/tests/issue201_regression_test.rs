use polyglot_sql::builder::{cast, col};
use polyglot_sql::dialects::{Dialect, DialectType};
use polyglot_sql::transpile;

#[test]
fn test_tsql_and_fabric_nvarchar_generation_differ_by_target() {
    let expr = cast(col("x"), "NVARCHAR(MAX)").into_inner();

    let tsql = Dialect::get(DialectType::TSQL).generate(&expr).unwrap();
    assert_eq!(tsql, "CAST(x AS NVARCHAR(MAX))");

    let fabric = Dialect::get(DialectType::Fabric).generate(&expr).unwrap();
    assert_eq!(fabric, "CAST(x AS VARCHAR(MAX))");
}

#[test]
fn test_tsql_preserves_nvarchar_in_cast_parsing() {
    let result = transpile(
        "SELECT CAST(x AS NVARCHAR(100))",
        DialectType::TSQL,
        DialectType::TSQL,
    )
    .unwrap();

    assert_eq!(result[0], "SELECT CAST(x AS NVARCHAR(100))");
}

#[test]
fn test_fabric_maps_nvarchar_to_supported_varchar() {
    let result = transpile(
        "SELECT CAST(x AS NVARCHAR(MAX))",
        DialectType::Fabric,
        DialectType::Fabric,
    )
    .unwrap();

    assert_eq!(result[0], "SELECT CAST(x AS VARCHAR(MAX))");
}
