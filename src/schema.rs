use tantivy::schema::{Schema, TEXT, STORED};

pub fn task_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("ID", TEXT | STORED);
    schema_builder.add_text_field("description", TEXT);
    schema_builder.add_text_field("project", TEXT);
    schema_builder.add_text_field("tags", TEXT);
    schema_builder.add_text_field("metadatas", TEXT);
    schema_builder.add_text_field("timetrack-annotations", TEXT);
    schema_builder.build()
}

pub fn note_schema() -> Schema {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("ID", TEXT | STORED);
    schema_builder.add_text_field("markdown", TEXT);
    schema_builder.add_text_field("metadatas", TEXT);
    schema_builder.build()
}

// eof
