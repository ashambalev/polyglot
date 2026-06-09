import pytest

import polyglot_sql


def test_analyze_query_returns_projection_facts():
    result = polyglot_sql.analyze_query("SELECT a FROM t")

    assert result["shape"] == "select"
    assert result["projections"][0]["name"] == "a"
    assert result["projections"][0]["transformKind"] == "direct"
    assert result["projections"][0]["upstream"][0]["column"] == "a"


def test_analyze_query_accepts_schema_options():
    schema = {
        "tables": [
            {
                "name": "orders",
                "columns": [
                    {"name": "total", "type": "INT"},
                    {"name": "user_id", "type": "INT"},
                ],
            }
        ]
    }
    result = polyglot_sql.analyze_query(
        "SELECT CAST(total AS TEXT) AS total_text FROM orders",
        {"schema": schema, "dialect": "generic"},
    )

    assert result["relations"][0]["name"] == "orders"
    assert "total" in result["relations"][0]["columns"]
    assert result["projections"][0]["transformKind"] == "cast"
    assert result["projections"][0]["castType"] == "TEXT"


def test_analyze_query_unknown_dialect_raises_value_error():
    with pytest.raises(ValueError):
        polyglot_sql.analyze_query("SELECT 1", dialect="not_a_dialect")


def test_analyze_query_rejects_invalid_options():
    with pytest.raises(ValueError):
        polyglot_sql.analyze_query("SELECT 1", "not an options object")
