use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STRING, STORED,
};

/// Builds the Tantivy schema for the nanosearch index.
///
/// Fields:
/// - `content`: full text of the file, indexed with default tokenizer, not stored
/// - `symbols`: extracted symbol names, indexed with custom "symbol" tokenizer, not stored
/// - `symbols_raw`: raw symbol string, untokenized and stored (for display)
/// - `path`: file path relative to repo root, untokenized and stored
/// - `lang`: detected language name, untokenized and stored
pub fn build_schema() -> Schema {
    let mut builder = Schema::builder();

    // content: TEXT indexed with default tokenizer, positions for BM25, not stored
    let content_options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    builder.add_text_field("content", content_options);

    // symbols: TEXT indexed with custom "symbol" tokenizer (whitespace + lowercase),
    // positions for BM25, not stored. The tokenizer itself is registered at index open time.
    let symbols_options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("symbol")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    builder.add_text_field("symbols", symbols_options);

    // symbols_raw: STRING (untokenized) | STORED
    // Intentionally STRING, not TEXT — stored as pipe-delimited "Foo|bar|Baz" for
    // post-retrieval splitting. Never searched directly; symbol search uses the
    // `symbols` TEXT field above. This avoids indexing overhead for a display-only field.
    builder.add_text_field("symbols_raw", STRING | STORED);

    // path: STRING (untokenized) | STORED — used for delete_term in incremental indexing
    builder.add_text_field("path", STRING | STORED);

    // lang: STRING (untokenized) | STORED
    builder.add_text_field("lang", STRING | STORED);

    builder.build()
}

/// Returns the `content` field handle.
pub fn content_field(schema: &Schema) -> Field {
    schema
        .get_field("content")
        .expect("schema missing 'content' field")
}

/// Returns the `symbols` field handle.
pub fn symbols_field(schema: &Schema) -> Field {
    schema
        .get_field("symbols")
        .expect("schema missing 'symbols' field")
}

/// Returns the `symbols_raw` field handle.
pub fn symbols_raw_field(schema: &Schema) -> Field {
    schema
        .get_field("symbols_raw")
        .expect("schema missing 'symbols_raw' field")
}

/// Returns the `path` field handle.
pub fn path_field(schema: &Schema) -> Field {
    schema
        .get_field("path")
        .expect("schema missing 'path' field")
}

/// Returns the `lang` field handle.
pub fn lang_field(schema: &Schema) -> Field {
    schema
        .get_field("lang")
        .expect("schema missing 'lang' field")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_has_five_fields() {
        let schema = build_schema();
        let fields: Vec<_> = schema.fields().collect();
        assert_eq!(fields.len(), 5, "schema should have exactly 5 fields");
    }

    #[test]
    fn field_helpers_resolve() {
        let schema = build_schema();
        // Each helper should return without panicking
        let _ = content_field(&schema);
        let _ = symbols_field(&schema);
        let _ = symbols_raw_field(&schema);
        let _ = path_field(&schema);
        let _ = lang_field(&schema);
    }
}
