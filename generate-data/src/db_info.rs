use markdown_tables::MarkdownTableRow;
use markdown_tables::as_table;
use rusqlite::{Connection, Result};
use std::collections::BTreeMap;

// Define structs to hold schema information
#[derive(Debug, Clone)]
pub(crate) struct SchemaObject {
    object_type: String,
    name: String,
    tbl_name: String,
    sql: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct Tables {
    pub(crate) tables: BTreeMap<String, Table>,
}

#[derive(Debug, Clone)]
pub(crate) struct Table {
    pub(crate) columns: Vec<TableColumn>,
}

#[derive(Debug, Clone)]
pub(crate) struct TableColumn {
    pub(crate) column_name: String,
    pub(crate) column_type: String,
    pub(crate) not_null: bool,
    pub(crate) default_value: Option<String>,
    pub(crate) is_primary_key: bool,
    pub(crate) is_autoincrement: bool,
    pub(crate) pointed_at_by: Vec<(String, String)>,
    pub(crate) pointing_to: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub(crate) struct ForeignKeyInfo {
    from_table: String,
    from_column: String,
    to_table: String,
    to_column: String,
}

impl MarkdownTableRow for SchemaObject {
    fn column_names() -> Vec<&'static str> {
        vec!["Type", "Name", "Table", "SQL"]
    }

    fn column_values(&self) -> Vec<String> {
        vec![
            self.object_type.clone(),
            self.name.clone(),
            self.tbl_name.clone(),
            self.sql.clone().unwrap_or_else(|| "NULL".to_string()),
        ]
    }
}

impl MarkdownTableRow for TableColumn {
    fn column_names() -> Vec<&'static str> {
        vec![
            "PK",
            "Column",
            "Type",
            "Not Null",
            "Default",
            "AutoInc",
            "Pointed At By",
            "Pointing To",
        ]
    }

    fn column_values(&self) -> Vec<String> {
        vec![
            if self.is_primary_key { "X" } else { "" }.to_string(),
            self.column_name.clone(),
            self.column_type.clone(),
            if self.not_null { "X" } else { "" }.to_string(),
            self.default_value.clone().unwrap_or_default(),
            if self.is_autoincrement { "X" } else { "" }.to_string(),
            self.pointed_at_by
                .iter()
                .map(|(table, column)| format!("← [{table}](#{table})/{column}"))
                .collect::<Vec<_>>()
                .join(", "),
            self.pointing_to
                .as_ref()
                .map(|(table, column)| format!("→ [{table}](#{table})/{column}"))
                .unwrap_or_default(),
        ]
    }
}

impl std::fmt::Display for Tables {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut unmentionable_tables = vec![];
        writeln!(f)?;
        writeln!(f, "## Tables")?;
        writeln!(f)?;
        for (table_name, Table { columns }) in &self.tables {
            if table_name.contains("indexed_") {
                unmentionable_tables.push(table_name.clone());
            } else {
                writeln!(f, "### {table_name}")?;
                writeln!(f, "{}", as_table(columns))?;
            }
        }

        writeln!(f, "## Additional Tables")?;
        for table_name in unmentionable_tables {
            writeln!(f, "- {table_name}")?;
        }
        Ok(())
    }
}

pub(crate) fn get_db_info(db: &Connection) -> Result<Tables> {
    // Query the schema table to get all objects
    let schema_objects: Vec<SchemaObject> = db
        .prepare(
            "SELECT type, name, tbl_name, sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
        )?
        .query_map([], |row| {
            Ok(SchemaObject {
                object_type: row.get(0)?,
                name: row.get(1)?,
                tbl_name: row.get(2)?,
                sql: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // get foreign keys
    let mut foreign_keys: Vec<ForeignKeyInfo> = Vec::new();
    for obj in &schema_objects {
        if obj.object_type == "table" {
            let fks = db
                .prepare(&format!("PRAGMA foreign_key_list('{}')", obj.name))?
                .query_map([], |row| {
                    Ok(ForeignKeyInfo {
                        from_table: obj.name.clone(),
                        from_column: row.get(3)?,
                        to_table: row.get(2)?,
                        to_column: row.get(4)?,
                    })
                })
                .inspect_err(|e| {
                    if cfg!(debug_assertions) {
                        panic!("Error getting foreign keys: {e}");
                    }
                    log::error!("Error getting foreign keys: {e}");
                })?
                .collect::<Result<Vec<_>, _>>()?;

            foreign_keys.extend(fks);
        }
    }

    // Get table columns
    let mut tables: BTreeMap<String, Table> = BTreeMap::new();
    for obj in &schema_objects {
        if obj.object_type == "table" {
            let table_name = obj.name.clone();

            let columns = db
                .prepare(&format!("PRAGMA table_info('{table_name}')"))?
                .query_map([], |row| {
                    let column_name = row.get::<_, String>(1)?;
                    let column_type = row.get::<_, String>(2)?;
                    let not_null = row.get::<_, i64>(3)? != 0;
                    let default_value = row.get(4)?;
                    let is_primary_key = row.get::<_, i64>(5)? != 0;

                    // Check if column is autoincrement by examining the table's SQL
                    // In SQLite, AUTOINCREMENT is only valid for INTEGER PRIMARY KEY columns
                    let is_autoincrement = if is_primary_key && column_type.to_uppercase() == "INTEGER" {
                        obj.sql.as_ref().is_some_and(|sql| {
                            let sql_upper = sql.to_uppercase();
                            // Look for various patterns of "INTEGER PRIMARY KEY AUTOINCREMENT"
                            // with the column name in different positions and with different quoting styles
                            sql_upper.contains("AUTOINCREMENT") && (
                                sql_upper.contains(&format!("{} INTEGER PRIMARY KEY AUTOINCREMENT", column_name.to_uppercase())) ||
                                sql_upper.contains(&format!("\"{}\" INTEGER PRIMARY KEY AUTOINCREMENT", column_name.to_uppercase())) ||
                                sql_upper.contains(&format!("'{}' INTEGER PRIMARY KEY AUTOINCREMENT", column_name.to_uppercase())) ||
                                sql_upper.contains(&format!("INTEGER PRIMARY KEY AUTOINCREMENT {}", column_name.to_uppercase())) ||
                                sql_upper.contains(&format!("INTEGER PRIMARY KEY AUTOINCREMENT \"{}\"", column_name.to_uppercase())) ||
                                sql_upper.contains(&format!("INTEGER PRIMARY KEY AUTOINCREMENT '{}'", column_name.to_uppercase())) ||
                                // Also check for the column being part of a multi-column primary key with AUTOINCREMENT
                                (sql_upper.contains(&format!("{} INTEGER", column_name.to_uppercase())) && 
                                 sql_upper.contains("PRIMARY KEY") && 
                                 sql_upper.contains("AUTOINCREMENT"))
                            )
                        })
                    } else {
                        false
                    };

                    Ok(TableColumn {
                        column_type,
                        not_null,
                        default_value,
                        is_primary_key,
                        is_autoincrement,
                        pointed_at_by: foreign_keys
                            .iter()
                            .filter(|fk| fk.to_table == table_name && fk.to_column == column_name)
                            .map(|fk| (fk.from_table.clone(), fk.from_column.clone()))
                            .collect(),
                        pointing_to: foreign_keys
                            .iter()
                            .find(|fk| fk.from_table == table_name && fk.from_column == column_name)
                            .map(|fk| (fk.to_table.clone(), fk.to_column.clone())),
                        column_name,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;

            tables.insert(table_name, Table { columns });
        }
    }
    let tables = Tables { tables };
    Ok(tables)
}
