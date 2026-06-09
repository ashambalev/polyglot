use polyglot_sql::{generate, parse, DialectType, Expression};

fn parse_one(sql: &str, dialect: DialectType) -> Expression {
    let mut expressions = parse(sql, dialect).expect("statement should parse");
    assert_eq!(expressions.len(), 1);
    expressions.remove(0)
}

fn assert_postgres_command(sql: &str, expected: &str) {
    let expr = parse_one(sql, DialectType::PostgreSQL);
    let Expression::Command(command) = &expr else {
        panic!("expected command expression, got {}", expr.variant_name());
    };

    assert_eq!(command.this, expected);
    assert_eq!(
        generate(&expr, DialectType::PostgreSQL).expect("command should generate"),
        expected
    );
}

#[test]
fn postgres_create_replication_slot_parses_as_command() {
    assert_postgres_command(
        r#"CREATE_REPLICATION_SLOT "sdp" LOGICAL pgoutput (SNAPSHOT 'nothing')"#,
        r#"CREATE_REPLICATION_SLOT "sdp" LOGICAL pgoutput(SNAPSHOT 'nothing')"#,
    );
}

#[test]
fn postgres_replication_protocol_commands_parse_as_commands() {
    for (sql, expected) in [
        (
            "BASE_BACKUP (LABEL 'polyglot')",
            "BASE_BACKUP(LABEL 'polyglot')",
        ),
        ("DROP_REPLICATION_SLOT sdp", "DROP_REPLICATION_SLOT sdp"),
        ("IDENTIFY_SYSTEM", "IDENTIFY_SYSTEM"),
        ("READ_REPLICATION_SLOT sdp", "READ_REPLICATION_SLOT sdp"),
        (
            "START_REPLICATION SLOT sdp LOGICAL 0/0",
            "START_REPLICATION SLOT sdp LOGICAL 0/0",
        ),
        ("TIMELINE_HISTORY 1", "TIMELINE_HISTORY 1"),
    ] {
        assert_postgres_command(sql, expected);
    }
}
