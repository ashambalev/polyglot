//! Regression tests for T-SQL parsing/generation and PostgreSQL -> T-SQL transpilation.

use polyglot_sql::generator::{Generator, GeneratorConfig};
use polyglot_sql::{
    get_all_tables, parse, transpile, DialectType, Expression, ExpressionWalk, Parser,
};

fn pg_to_tsql(sql: &str) -> String {
    transpile(sql, DialectType::PostgreSQL, DialectType::TSQL)
        .unwrap_or_else(|e| panic!("transpile failed for {sql:?}: {e}"))
        .into_iter()
        .next()
        .expect("expected at least one statement")
}

fn generate_tsql(expr: &Expression) -> String {
    let config = GeneratorConfig {
        dialect: Some(DialectType::TSQL),
        ..Default::default()
    };
    let mut generator = Generator::with_config(config);
    generator
        .generate(expr)
        .expect("expression should generate as T-SQL")
}

const TRY_CATCH_SQL: &str = r#"BEGIN TRY
    INSERT INTO orders (id, amount) VALUES (1, 100.00);
    UPDATE inventory SET qty = qty - 1 WHERE product_id = 42;
END TRY
BEGIN CATCH
    INSERT INTO error_log (msg) VALUES (ERROR_MESSAGE());
END CATCH"#;

// ---------------------------------------------------------------------------
// T-SQL TRY/CATCH structured traversal
// ---------------------------------------------------------------------------

#[test]
fn try_catch_parses_structured_bodies_and_generates_sql() {
    let ast = Parser::parse_sql(TRY_CATCH_SQL).expect("TRY/CATCH should parse");
    assert_eq!(ast.len(), 1);

    let Expression::TryCatch(try_catch) = &ast[0] else {
        panic!("expected TRY/CATCH expression, got {:?}", ast[0]);
    };

    assert_eq!(try_catch.try_body.len(), 2);
    assert_eq!(try_catch.catch_body.as_ref().map(Vec::len), Some(1));

    let sql = Generator::sql(&ast[0]).expect("TRY/CATCH should generate");
    assert_eq!(
        sql,
        "BEGIN TRY INSERT INTO orders (id, amount) VALUES (1, 100.00); UPDATE inventory SET qty = qty - 1 WHERE product_id = 42; END TRY BEGIN CATCH INSERT INTO error_log (msg) VALUES (ERROR_MESSAGE()); END CATCH"
    );
}

#[test]
fn try_catch_children_include_inner_statements() {
    let ast = Parser::parse_sql(TRY_CATCH_SQL).expect("TRY/CATCH should parse");
    let children = ast[0].children();

    assert_eq!(children.len(), 3);
    assert!(matches!(children[0], Expression::Insert(_)));
    assert!(matches!(children[1], Expression::Update(_)));
    assert!(matches!(children[2], Expression::Insert(_)));
}

#[test]
fn try_catch_get_all_tables_finds_try_and_catch_tables() {
    let ast = Parser::parse_sql(TRY_CATCH_SQL).expect("TRY/CATCH should parse");
    let names: Vec<String> = get_all_tables(&ast[0])
        .into_iter()
        .filter_map(|table| match table {
            Expression::Table(table) => Some(table.name.name),
            _ => None,
        })
        .collect();

    assert_eq!(names, vec!["orders", "inventory", "error_log"]);
}

// ---------------------------------------------------------------------------
// DECLARE statement boundaries
// ---------------------------------------------------------------------------

#[test]
fn declare_table_variable_keeps_following_insert_as_second_statement() {
    let sql = "DECLARE @tmp TABLE (id INT, name VARCHAR(50)); \
               INSERT INTO @tmp SELECT id, name FROM employees;";
    let ast = parse(sql, DialectType::TSQL).expect("DECLARE TABLE batch should parse");

    assert_eq!(ast.len(), 2);
    assert!(matches!(ast[0], Expression::Declare(_)));
    assert!(matches!(ast[1], Expression::Insert(_)));
    assert_eq!(
        generate_tsql(&ast[0]),
        "DECLARE @tmp TABLE (id INTEGER, name VARCHAR(50))"
    );
    assert_eq!(
        generate_tsql(&ast[1]),
        "INSERT INTO @tmp SELECT id, name FROM employees"
    );

    let names: Vec<String> = get_all_tables(&ast[1])
        .into_iter()
        .filter_map(|table| match table {
            Expression::Table(table) => Some(table.name.name),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["@tmp", "employees"]);
}

#[test]
fn declare_scalar_keeps_following_select_as_second_statement() {
    let ast = parse("DECLARE @x INT; SELECT @x;", DialectType::TSQL)
        .expect("DECLARE scalar batch should parse");

    assert_eq!(ast.len(), 2);
    assert!(matches!(ast[0], Expression::Declare(_)));
    assert!(matches!(ast[1], Expression::Select(_)));
    assert_eq!(generate_tsql(&ast[0]), "DECLARE @x INTEGER");
    assert_eq!(generate_tsql(&ast[1]), "SELECT @x");
}

// ---------------------------------------------------------------------------
// BPCHAR → CHAR normalisation
// ---------------------------------------------------------------------------

#[test]
fn bpchar_cast_no_length_maps_to_char() {
    let out = pg_to_tsql("SELECT CAST(x AS BPCHAR)");
    assert_eq!(out, "SELECT CAST(x AS CHAR)");
}

#[test]
fn bpchar_cast_with_length_maps_to_char() {
    let out = pg_to_tsql("SELECT CAST(x AS BPCHAR(3))");
    assert_eq!(out, "SELECT CAST(x AS CHAR(3))");
}

#[test]
fn bpchar_double_colon_no_length_maps_to_char() {
    let out = pg_to_tsql("SELECT x::bpchar");
    assert_eq!(out, "SELECT CAST(x AS CHAR)");
}

#[test]
fn bpchar_double_colon_with_length_maps_to_char() {
    let out = pg_to_tsql("SELECT x::bpchar(3)");
    assert_eq!(out, "SELECT CAST(x AS CHAR(3))");
}

#[test]
fn bpchar_ddl_column_no_length_maps_to_char() {
    let out = pg_to_tsql("CREATE TABLE t (x BPCHAR)");
    assert_eq!(out, "CREATE TABLE t (x CHAR)");
}

#[test]
fn bpchar_ddl_column_with_length_maps_to_char() {
    let out = pg_to_tsql("CREATE TABLE t (x BPCHAR(3))");
    assert_eq!(out, "CREATE TABLE t (x CHAR(3))");
}

// ---------------------------------------------------------------------------
// = ANY(ARRAY[...]) / = ANY((...)) → IN
// ---------------------------------------------------------------------------

#[test]
fn any_eq_array_brackets_rewrites_to_in() {
    let out = pg_to_tsql("SELECT * FROM t WHERE col = ANY(ARRAY['a', 'b', 'c'])");
    assert_eq!(out, "SELECT * FROM t WHERE col IN ('a', 'b', 'c')");
}

#[test]
fn any_eq_tuple_rewrites_to_in() {
    let out = pg_to_tsql("SELECT * FROM t WHERE col = ANY(('a', 'b', 'c'))");
    assert_eq!(out, "SELECT * FROM t WHERE col IN ('a', 'b', 'c')");
}

#[test]
fn any_eq_empty_array_rewrites_to_always_false() {
    let out = pg_to_tsql("SELECT * FROM t WHERE col = ANY(ARRAY[])");
    assert_eq!(out, "SELECT * FROM t WHERE 1 = 0");
}

#[test]
fn any_neq_array_not_rewritten() {
    let out = pg_to_tsql("SELECT * FROM t WHERE col <> ANY(ARRAY['a', 'b'])");
    assert_eq!(out, "SELECT * FROM t WHERE col <> ANY(ARRAY['a', 'b'])");
}

#[test]
fn any_eq_subquery_not_rewritten() {
    let out = pg_to_tsql("SELECT * FROM t WHERE col = ANY(SELECT id FROM s)");
    assert_eq!(out, "SELECT * FROM t WHERE col = ANY (SELECT id FROM s)");
}
