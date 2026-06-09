use polyglot_sql::{generate_data_type, parse_data_type, DataType, DialectType};

#[test]
fn parse_standalone_decimal_type() {
    let data_type =
        parse_data_type("DECIMAL(10, 2)", DialectType::DuckDB).expect("decimal should parse");

    assert_eq!(
        data_type,
        DataType::Decimal {
            precision: Some(10),
            scale: Some(2),
        }
    );
}

#[test]
fn render_standalone_data_type_for_target_dialect() {
    let data_type =
        parse_data_type("VARCHAR(255)", DialectType::DuckDB).expect("varchar should parse");

    assert_eq!(
        generate_data_type(&data_type, DialectType::DuckDB).expect("duckdb render"),
        "TEXT(255)"
    );
    assert_eq!(
        generate_data_type(&data_type, DialectType::PostgreSQL).expect("postgres render"),
        "VARCHAR(255)"
    );
}

#[test]
fn parse_standalone_array_type() {
    let data_type = parse_data_type("INT[]", DialectType::DuckDB).expect("array should parse");

    match data_type {
        DataType::Array {
            element_type,
            dimension,
        } => {
            assert_eq!(
                *element_type,
                DataType::Int {
                    length: None,
                    integer_spelling: false,
                }
            );
            assert_eq!(dimension, None);
        }
        other => panic!("expected array data type, got {other:?}"),
    }
}

#[test]
fn parse_standalone_struct_type() {
    let data_type = parse_data_type("STRUCT(a INT, b VARCHAR)", DialectType::DuckDB)
        .expect("struct should parse");

    assert_eq!(
        generate_data_type(&data_type, DialectType::DuckDB).expect("duckdb struct render"),
        "STRUCT(a INT, b TEXT)"
    );
}

#[test]
fn parse_standalone_custom_type_preserves_name() {
    let data_type =
        parse_data_type("MyCustomType", DialectType::DuckDB).expect("custom type should parse");

    assert_eq!(
        data_type,
        DataType::Custom {
            name: "MyCustomType".to_string(),
        }
    );
}

#[test]
fn parse_standalone_data_type_rejects_trailing_sql() {
    let error = parse_data_type("DECIMAL(10, 2) SELECT 1", DialectType::DuckDB)
        .expect_err("trailing SQL should fail");

    assert!(error
        .to_string()
        .contains("Unexpected token after data type"));
}
