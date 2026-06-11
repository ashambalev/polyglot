//! Compact query analysis facts.
//!
//! This module intentionally builds on the existing parser, scope builder, type
//! annotator, and lineage implementation. It is a convenience API: callers that
//! need the full AST or full lineage graph should continue using those lower
//! level APIs directly.

use crate::ast_transforms::get_output_column_names;
use crate::dialects::{Dialect, DialectType};
use crate::expressions::{DataType, Expression, TableRef, With};
use crate::lineage::{lineage_by_index_from_expression, LineageNode};
use crate::optimizer::annotate_types::annotate_types;
use crate::optimizer::qualify_columns::{qualify_columns, QualifyColumnsOptions};
use crate::schema::{MappingSchema, Schema};
use crate::scope::{build_scope, Scope, SourceInfo, SourceKind};
use crate::traversal::{contains_aggregate, ExpressionWalk};
use crate::validation::{mapping_schema_from_validation_schema, ValidationSchema};
use crate::{parse_data_type, parse_one, Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Options for [`analyze_query`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AnalyzeQueryOptions {
    /// SQL dialect used for parsing and dialect-aware rendering.
    pub dialect: DialectType,
    /// Optional validation schema used for qualification and type annotation.
    pub schema: Option<ValidationSchema>,
}

/// Compact facts about a query's output shape and data dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryAnalysis {
    pub shape: QueryShape,
    pub ctes: Vec<String>,
    pub projections: Vec<ProjectionFact>,
    pub relations: Vec<RelationFact>,
    pub base_tables: Vec<RelationFact>,
    pub set_operations: Vec<SetOperationFact>,
}

/// Top-level query shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryShape {
    Select,
    SetOperation,
}

/// Compact fact about one output projection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionFact {
    pub index: usize,
    pub name: Option<String>,
    pub is_star: bool,
    pub star_table: Option<String>,
    pub transform_kind: TransformKind,
    pub cast_type: Option<String>,
    pub type_hint: Option<String>,
    pub upstream: Vec<ColumnReferenceFact>,
}

/// Compact fact about an upstream column reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnReferenceFact {
    pub source_name: Option<String>,
    pub source_alias: Option<String>,
    pub source_kind: SourceKind,
    pub table: Option<String>,
    pub column: String,
    pub unqualified: bool,
    pub confidence: ReferenceConfidence,
}

/// Compact fact about a relation visible in the root scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationFact {
    pub name: String,
    pub alias: Option<String>,
    pub kind: SourceKind,
    pub columns: Vec<String>,
}

/// Compact fact about a set operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOperationFact {
    pub kind: String,
    pub all: bool,
    pub distinct: bool,
    pub output_columns: Vec<String>,
    pub branches: Vec<SetOperationBranchFact>,
}

/// Compact facts for one immediate set-operation branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOperationBranchFact {
    pub index: usize,
    pub projections: Vec<ProjectionFact>,
}

/// High-level kind of transformation performed by a projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformKind {
    Direct,
    Cast,
    Aggregation,
    Constant,
    Expression,
    Star,
}

/// Confidence level for a compact upstream column reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceConfidence {
    Resolved,
    Ambiguous,
    Unknown,
}

/// Analyze a single SELECT or set-operation query.
pub fn analyze_query(sql: &str, options: AnalyzeQueryOptions) -> Result<QueryAnalysis> {
    let mut expression = parse_one(sql, options.dialect)?;
    expression = effective_query(expression);
    ensure_query(&expression)?;

    let mapping_schema = options
        .schema
        .as_ref()
        .map(|schema| analysis_mapping_schema(schema, options.dialect));

    if let Some(schema) = mapping_schema.as_ref() {
        let qualify_options = QualifyColumnsOptions::new().with_dialect(options.dialect);
        expression = qualify_columns(expression, schema, &qualify_options)
            .map_err(|e| Error::internal(format!("query analysis qualification failed: {e}")))?;
    }

    let annotation_schema = mapping_schema.as_ref().map(|schema| {
        let mut alias_schema = schema.clone();
        add_scope_aliases_to_schema(
            &build_scope(&expression),
            schema,
            &mut alias_schema,
            options.dialect,
        );
        alias_schema
    });

    annotate_types(
        &mut expression,
        annotation_schema
            .as_ref()
            .map(|schema| schema as &dyn Schema),
        Some(options.dialect),
    );
    crate::lineage::expand_cte_stars(
        &mut expression,
        annotation_schema
            .as_ref()
            .or(mapping_schema.as_ref())
            .map(|schema| schema as &dyn Schema),
    );

    let scope = build_scope(&expression);
    let shape = if is_set_operation(&expression) {
        QueryShape::SetOperation
    } else {
        QueryShape::Select
    };

    Ok(QueryAnalysis {
        shape,
        ctes: collect_cte_names(&expression),
        projections: projection_facts_for_query(&expression, &scope, options.dialect),
        relations: relation_facts(&scope, mapping_schema.as_ref()),
        base_tables: base_table_facts(&scope, mapping_schema.as_ref()),
        set_operations: set_operation_facts(&expression, &scope, options.dialect),
    })
}

fn analysis_mapping_schema(schema: &ValidationSchema, dialect: DialectType) -> MappingSchema {
    let broad_schema = mapping_schema_from_validation_schema(schema);
    let mut mapping_schema = MappingSchema::with_dialect(dialect);

    for table in &schema.tables {
        let table_names = validation_table_names(table);
        if table_names.is_empty() {
            continue;
        }

        let fallback_table = table_names[0].as_str();
        let columns: Vec<(String, DataType)> = table
            .columns
            .iter()
            .map(|column| {
                let data_type = parse_analysis_data_type(&column.data_type, dialect)
                    .unwrap_or_else(|| {
                        broad_schema
                            .get_column_type(fallback_table, &column.name)
                            .unwrap_or(DataType::Unknown)
                    });
                (column.name.to_ascii_lowercase(), data_type)
            })
            .collect();

        for table_name in table_names {
            let _ = mapping_schema.add_table(&table_name, &columns, Some(dialect));
        }
    }

    mapping_schema
}

fn validation_table_names(table: &crate::validation::SchemaTable) -> Vec<String> {
    let mut names = Vec::new();

    names.push(table.name.to_ascii_lowercase());
    if let Some(schema_name) = &table.schema {
        names.push(format!(
            "{}.{}",
            schema_name.to_ascii_lowercase(),
            table.name.to_ascii_lowercase()
        ));
    }
    for alias in &table.aliases {
        names.push(alias.to_ascii_lowercase());
    }

    names.sort();
    names.dedup();
    names
}

fn parse_analysis_data_type(data_type: &str, dialect: DialectType) -> Option<DataType> {
    let trimmed = data_type.trim();
    if trimmed.is_empty() {
        return None;
    }
    parse_data_type(trimmed, dialect).ok()
}

fn add_scope_aliases_to_schema(
    scope: &Scope,
    source_schema: &MappingSchema,
    target_schema: &mut MappingSchema,
    dialect: DialectType,
) {
    for child_scope in scope.traverse() {
        for (source_name, source) in &child_scope.sources {
            if source.kind != SourceKind::Table {
                continue;
            }
            if let Some(table_name) = source_table_name(source) {
                if source_name == &table_name {
                    continue;
                }
                if let Ok(column_names) = source_schema.column_names(&table_name) {
                    let columns: Vec<(String, DataType)> = column_names
                        .iter()
                        .map(|column| {
                            (
                                column.clone(),
                                source_schema
                                    .get_column_type(&table_name, column)
                                    .unwrap_or(DataType::Unknown),
                            )
                        })
                        .collect();
                    let _ = target_schema.add_table(source_name, &columns, Some(dialect));
                }
            }
        }
    }
}

fn effective_query(expression: Expression) -> Expression {
    match expression {
        Expression::Prepare(prepare) => prepare.statement,
        Expression::Subquery(subquery) if subquery.alias.is_none() => subquery.this,
        other => other,
    }
}

fn ensure_query(expression: &Expression) -> Result<()> {
    if matches!(
        expression,
        Expression::Select(_)
            | Expression::Union(_)
            | Expression::Intersect(_)
            | Expression::Except(_)
    ) {
        Ok(())
    } else {
        Err(Error::internal(
            "analyze_query requires a SELECT or set operation query",
        ))
    }
}

fn is_set_operation(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Union(_) | Expression::Intersect(_) | Expression::Except(_)
    )
}

fn collect_cte_names(expression: &Expression) -> Vec<String> {
    let mut names = Vec::new();
    let mut seen = HashSet::new();
    collect_cte_names_inner(expression, &mut names, &mut seen);
    names
}

fn collect_cte_names_inner(
    expression: &Expression,
    names: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if let Some(with_clause) = with_clause(expression) {
        collect_with_names(with_clause, names, seen);
    }

    match expression {
        Expression::Union(union) => {
            collect_cte_names_inner(&union.left, names, seen);
            collect_cte_names_inner(&union.right, names, seen);
        }
        Expression::Intersect(intersect) => {
            collect_cte_names_inner(&intersect.left, names, seen);
            collect_cte_names_inner(&intersect.right, names, seen);
        }
        Expression::Except(except) => {
            collect_cte_names_inner(&except.left, names, seen);
            collect_cte_names_inner(&except.right, names, seen);
        }
        Expression::Subquery(subquery) => collect_cte_names_inner(&subquery.this, names, seen),
        _ => {}
    }
}

fn collect_with_names(with_clause: &With, names: &mut Vec<String>, seen: &mut HashSet<String>) {
    for cte in &with_clause.ctes {
        if seen.insert(cte.alias.name.clone()) {
            names.push(cte.alias.name.clone());
        }
        collect_cte_names_inner(&cte.this, names, seen);
    }
}

fn with_clause(expression: &Expression) -> Option<&With> {
    match expression {
        Expression::Select(select) => select.with.as_ref(),
        Expression::Union(union) => union.with.as_ref(),
        Expression::Intersect(intersect) => intersect.with.as_ref(),
        Expression::Except(except) => except.with.as_ref(),
        _ => None,
    }
}

fn projection_facts_for_query(
    expression: &Expression,
    scope: &Scope,
    dialect: DialectType,
) -> Vec<ProjectionFact> {
    let expressions = select_expressions_for_query(expression);
    let names = get_output_column_names(expression);

    expressions
        .iter()
        .enumerate()
        .map(|(index, projection)| {
            projection_fact(
                index,
                names
                    .get(index)
                    .cloned()
                    .or_else(|| projection_name(projection)),
                projection,
                expression,
                scope,
                dialect,
            )
        })
        .collect()
}

fn select_expressions_for_query(expression: &Expression) -> Vec<&Expression> {
    match expression {
        Expression::Select(select) => select.expressions.iter().collect(),
        Expression::Union(union) => select_expressions_for_query(&union.left),
        Expression::Intersect(intersect) => select_expressions_for_query(&intersect.left),
        Expression::Except(except) => select_expressions_for_query(&except.left),
        Expression::Subquery(subquery) => select_expressions_for_query(&subquery.this),
        _ => Vec::new(),
    }
}

fn projection_fact(
    index: usize,
    name: Option<String>,
    projection: &Expression,
    query: &Expression,
    scope: &Scope,
    dialect: DialectType,
) -> ProjectionFact {
    let inner = unwrap_projection_alias(projection);
    let is_star = projection_is_star(inner);
    let upstream = lineage_by_index_from_expression(index, query, Some(dialect), false)
        .map(|node| terminal_references_from_lineage(&node))
        .ok()
        .filter(|refs| !refs.is_empty())
        .unwrap_or_else(|| fallback_column_references(inner, scope));

    ProjectionFact {
        index,
        name,
        is_star,
        star_table: projection_star_table(inner),
        transform_kind: transform_kind(inner),
        cast_type: cast_type(inner, dialect),
        type_hint: projection
            .inferred_type()
            .or_else(|| inner.inferred_type())
            .and_then(|data_type| render_data_type(data_type, dialect)),
        upstream,
    }
}

fn unwrap_projection_alias(expression: &Expression) -> &Expression {
    match expression {
        Expression::Alias(alias) => unwrap_projection_alias(&alias.this),
        Expression::Annotated(annotated) => unwrap_projection_alias(&annotated.this),
        Expression::Paren(paren) => unwrap_projection_alias(&paren.this),
        _ => expression,
    }
}

fn projection_name(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Alias(alias) => Some(alias.alias.name.clone()),
        Expression::Column(column) => Some(column.name.name.clone()),
        Expression::Identifier(identifier) => Some(identifier.name.clone()),
        Expression::Star(_) => Some("*".to_string()),
        Expression::Annotated(annotated) => projection_name(&annotated.this),
        _ => None,
    }
}

fn projection_is_star(expression: &Expression) -> bool {
    matches!(expression, Expression::Star(_))
        || matches!(expression, Expression::Column(column) if column.name.name == "*")
}

fn projection_star_table(expression: &Expression) -> Option<String> {
    match expression {
        Expression::Star(star) => star
            .table
            .as_ref()
            .map(|identifier| identifier.name.clone()),
        Expression::Column(column) if column.name.name == "*" => column
            .table
            .as_ref()
            .map(|identifier| identifier.name.clone()),
        _ => None,
    }
}

fn transform_kind(expression: &Expression) -> TransformKind {
    if projection_is_star(expression) {
        TransformKind::Star
    } else if is_cast_expression(expression) {
        TransformKind::Cast
    } else if contains_aggregate(expression) {
        TransformKind::Aggregation
    } else if matches!(
        expression,
        Expression::Column(_) | Expression::Identifier(_)
    ) {
        TransformKind::Direct
    } else if is_simple_constant(expression) {
        TransformKind::Constant
    } else {
        TransformKind::Expression
    }
}

fn is_cast_expression(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Cast(_) | Expression::TryCast(_) | Expression::SafeCast(_)
    )
}

fn cast_type(expression: &Expression, dialect: DialectType) -> Option<String> {
    match expression {
        Expression::Cast(cast) | Expression::TryCast(cast) | Expression::SafeCast(cast) => {
            render_data_type(&cast.to, dialect)
        }
        _ => None,
    }
}

fn render_data_type(data_type: &DataType, dialect: DialectType) -> Option<String> {
    Dialect::get(dialect)
        .generate(&Expression::DataType(data_type.clone()))
        .ok()
}

fn is_simple_constant(expression: &Expression) -> bool {
    match expression {
        Expression::Literal(_) | Expression::Boolean(_) | Expression::Null(_) => true,
        Expression::Cast(cast) | Expression::TryCast(cast) | Expression::SafeCast(cast) => {
            is_simple_constant(&cast.this)
        }
        Expression::Neg(unary) | Expression::BitwiseNot(unary) => is_simple_constant(&unary.this),
        _ => false,
    }
}

fn terminal_references_from_lineage(node: &LineageNode) -> Vec<ColumnReferenceFact> {
    let mut refs = Vec::new();
    collect_terminal_references(node, &mut refs);
    dedupe_column_refs(refs)
}

fn collect_terminal_references(node: &LineageNode, refs: &mut Vec<ColumnReferenceFact>) {
    if node.downstream.is_empty() {
        if let Some(reference) = column_reference_from_lineage_node(node) {
            refs.push(reference);
        }
        return;
    }

    for child in &node.downstream {
        collect_terminal_references(child, refs);
    }
}

fn column_reference_from_lineage_node(node: &LineageNode) -> Option<ColumnReferenceFact> {
    match &node.expression {
        Expression::Column(column) => {
            let source_name = non_empty_string(node.source_name.clone());
            let table =
                lineage_node_table(node).or_else(|| column.table.as_ref().map(|t| t.name.clone()));
            let confidence = if node.source_kind == SourceKind::Unknown && source_name.is_none() {
                ReferenceConfidence::Unknown
            } else {
                ReferenceConfidence::Resolved
            };
            Some(ColumnReferenceFact {
                source_name,
                source_alias: node.source_alias.clone(),
                source_kind: node.source_kind,
                table,
                column: column.name.name.clone(),
                unqualified: column.table.is_none(),
                confidence,
            })
        }
        Expression::Star(_) => Some(ColumnReferenceFact {
            source_name: non_empty_string(node.source_name.clone()),
            source_alias: node.source_alias.clone(),
            source_kind: node.source_kind,
            table: lineage_node_table(node),
            column: "*".to_string(),
            unqualified: true,
            confidence: if node.source_kind == SourceKind::Unknown {
                ReferenceConfidence::Unknown
            } else {
                ReferenceConfidence::Resolved
            },
        }),
        _ => None,
    }
}

fn lineage_node_table(node: &LineageNode) -> Option<String> {
    match &node.source {
        Expression::Table(table) => Some(table_name(table)),
        _ => None,
    }
}

fn fallback_column_references(expression: &Expression, scope: &Scope) -> Vec<ColumnReferenceFact> {
    let mut refs = Vec::new();
    let source_count = scope.sources.len();
    let single_source = if source_count == 1 {
        scope.sources.iter().next()
    } else {
        None
    };

    for column_expr in expression.find_all(|candidate| matches!(candidate, Expression::Column(_))) {
        if let Expression::Column(column) = column_expr {
            if column.name.name == "*" {
                continue;
            }
            let source = column
                .table
                .as_ref()
                .and_then(|table| scope.sources.get(&table.name));
            let (source_name, source_alias, source_kind, table, confidence) =
                if let Some(table_identifier) = &column.table {
                    if let Some(source) = source {
                        (
                            Some(table_identifier.name.clone()),
                            source.alias.clone(),
                            source.kind,
                            source_table_name(source)
                                .or_else(|| Some(table_identifier.name.clone())),
                            ReferenceConfidence::Resolved,
                        )
                    } else {
                        (
                            Some(table_identifier.name.clone()),
                            None,
                            SourceKind::Unknown,
                            Some(table_identifier.name.clone()),
                            ReferenceConfidence::Unknown,
                        )
                    }
                } else if let Some((name, source)) = single_source {
                    (
                        Some(name.clone()),
                        source.alias.clone(),
                        source.kind,
                        source_table_name(source).or_else(|| Some(name.clone())),
                        ReferenceConfidence::Resolved,
                    )
                } else if source_count > 1 {
                    (
                        None,
                        None,
                        SourceKind::Unknown,
                        None,
                        ReferenceConfidence::Ambiguous,
                    )
                } else {
                    (
                        None,
                        None,
                        SourceKind::Unknown,
                        None,
                        ReferenceConfidence::Unknown,
                    )
                };

            refs.push(ColumnReferenceFact {
                source_name,
                source_alias,
                source_kind,
                table,
                column: column.name.name.clone(),
                unqualified: column.table.is_none(),
                confidence,
            });
        }
    }

    dedupe_column_refs(refs)
}

fn dedupe_column_refs(refs: Vec<ColumnReferenceFact>) -> Vec<ColumnReferenceFact> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for reference in refs {
        let key = (
            reference.source_name.clone(),
            reference.source_alias.clone(),
            reference.table.clone(),
            reference.column.clone(),
            format!("{:?}", reference.source_kind),
            reference.unqualified,
            format!("{:?}", reference.confidence),
        );
        if seen.insert(key) {
            deduped.push(reference);
        }
    }

    deduped
}

fn relation_facts(
    scope: &Scope,
    mapping_schema: Option<&crate::schema::MappingSchema>,
) -> Vec<RelationFact> {
    let mut relations = Vec::new();
    let mut seen = HashSet::new();
    collect_relation_facts(scope, mapping_schema, &mut seen, &mut relations);

    relations.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.alias.cmp(&right.alias))
    });
    relations
}

fn collect_relation_facts(
    scope: &Scope,
    mapping_schema: Option<&crate::schema::MappingSchema>,
    seen: &mut HashSet<String>,
    relations: &mut Vec<RelationFact>,
) {
    for relation in scope
        .sources
        .iter()
        .map(|(source_name, source)| RelationFact {
            name: source
                .lineage_name
                .clone()
                .or_else(|| source_table_name(source))
                .unwrap_or_else(|| source_name.clone()),
            alias: source.alias.clone().or_else(|| source_alias(source)),
            kind: source.kind,
            columns: source_columns(source, mapping_schema),
        })
    {
        let key = format!("{:?}|{}|{:?}", relation.kind, relation.name, relation.alias);
        if seen.insert(key) {
            relations.push(relation);
        }
    }

    for branch_scope in &scope.union_scopes {
        collect_relation_facts(branch_scope, mapping_schema, seen, relations);
    }
}

fn base_table_facts(
    scope: &Scope,
    mapping_schema: Option<&crate::schema::MappingSchema>,
) -> Vec<RelationFact> {
    let mut relations = Vec::new();
    let mut seen = HashSet::new();

    for child_scope in scope.traverse() {
        for source in child_scope.sources.values() {
            if source.kind != SourceKind::Table {
                continue;
            }

            let Some(table_name) = source_table_name(source) else {
                continue;
            };

            if seen.insert(table_name.clone()) {
                relations.push(RelationFact {
                    name: table_name,
                    alias: source.alias.clone().or_else(|| source_alias(source)),
                    kind: SourceKind::Table,
                    columns: source_columns(source, mapping_schema),
                });
            }
        }
    }

    relations.sort_by(|left, right| left.name.cmp(&right.name));
    relations
}

fn source_columns(
    source: &SourceInfo,
    mapping_schema: Option<&crate::schema::MappingSchema>,
) -> Vec<String> {
    match &source.expression {
        Expression::Table(table) => mapping_schema
            .and_then(|schema| schema.column_names(&table_name(table)).ok())
            .unwrap_or_default(),
        Expression::Select(_)
        | Expression::Union(_)
        | Expression::Intersect(_)
        | Expression::Except(_) => get_output_column_names(&source.expression),
        Expression::Subquery(subquery) => get_output_column_names(&subquery.this),
        Expression::Cte(cte) if !cte.columns.is_empty() => cte
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect(),
        Expression::Cte(cte) => get_output_column_names(&cte.this),
        _ => Vec::new(),
    }
}

fn source_table_name(source: &SourceInfo) -> Option<String> {
    match &source.expression {
        Expression::Table(table) => Some(table_name(table)),
        _ => None,
    }
}

fn source_alias(source: &SourceInfo) -> Option<String> {
    match &source.expression {
        Expression::Table(table) => table.alias.as_ref().map(|alias| alias.name.clone()),
        Expression::Subquery(subquery) => subquery.alias.as_ref().map(|alias| alias.name.clone()),
        _ => None,
    }
}

fn table_name(table: &TableRef) -> String {
    let mut parts = Vec::new();
    if let Some(catalog) = &table.catalog {
        parts.push(catalog.name.clone());
    }
    if let Some(schema) = &table.schema {
        parts.push(schema.name.clone());
    }
    parts.push(table.name.name.clone());
    parts.join(".")
}

fn set_operation_facts(
    expression: &Expression,
    scope: &Scope,
    dialect: DialectType,
) -> Vec<SetOperationFact> {
    let mut facts = Vec::new();
    collect_set_operation_facts(expression, scope, dialect, &mut facts);
    facts
}

fn collect_set_operation_facts(
    expression: &Expression,
    scope: &Scope,
    dialect: DialectType,
    facts: &mut Vec<SetOperationFact>,
) {
    match expression {
        Expression::Union(union) => {
            facts.push(SetOperationFact {
                kind: "union".to_string(),
                all: union.all,
                distinct: union.distinct,
                output_columns: get_output_column_names(expression),
                branches: set_operation_branches(&union.left, &union.right, scope, dialect),
            });
            collect_set_operation_facts(&union.left, scope, dialect, facts);
            collect_set_operation_facts(&union.right, scope, dialect, facts);
        }
        Expression::Intersect(intersect) => {
            facts.push(SetOperationFact {
                kind: "intersect".to_string(),
                all: intersect.all,
                distinct: intersect.distinct,
                output_columns: get_output_column_names(expression),
                branches: set_operation_branches(&intersect.left, &intersect.right, scope, dialect),
            });
            collect_set_operation_facts(&intersect.left, scope, dialect, facts);
            collect_set_operation_facts(&intersect.right, scope, dialect, facts);
        }
        Expression::Except(except) => {
            facts.push(SetOperationFact {
                kind: "except".to_string(),
                all: except.all,
                distinct: except.distinct,
                output_columns: get_output_column_names(expression),
                branches: set_operation_branches(&except.left, &except.right, scope, dialect),
            });
            collect_set_operation_facts(&except.left, scope, dialect, facts);
            collect_set_operation_facts(&except.right, scope, dialect, facts);
        }
        Expression::Subquery(subquery) => {
            collect_set_operation_facts(&subquery.this, scope, dialect, facts);
        }
        _ => {}
    }
}

fn set_operation_branches(
    left: &Expression,
    right: &Expression,
    scope: &Scope,
    dialect: DialectType,
) -> Vec<SetOperationBranchFact> {
    vec![
        SetOperationBranchFact {
            index: 0,
            projections: projection_facts_for_branch(left, scope, dialect),
        },
        SetOperationBranchFact {
            index: 1,
            projections: projection_facts_for_branch(right, scope, dialect),
        },
    ]
}

fn projection_facts_for_branch(
    expression: &Expression,
    root_scope: &Scope,
    dialect: DialectType,
) -> Vec<ProjectionFact> {
    let branch_scope = build_scope(expression);
    let scope = if branch_scope.sources.is_empty() {
        root_scope
    } else {
        &branch_scope
    };
    projection_facts_for_query(expression, scope, dialect)
}

fn non_empty_string(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
