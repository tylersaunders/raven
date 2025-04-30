use super::SqlString;
use std::fmt::Write as _;

/// Delete rows from a table
#[derive(Default, Debug, Clone)]
pub struct DeleteStatement<'a> {
    pub table: &'a str,
    pub r#where: Vec<(&'a str, Option<&'a str>)>, // Reuse the where clause logic
}

impl SqlString for DeleteStatement<'_> {
    /// Convert the [`DeleteStatement`] into a runnable SQL string.
    fn to_sql(&self) -> String {
        let mut sql = String::new();
        // sql.push_str(&format!("DELETE FROM {}", self.table));
        let _ = write!(sql, "DELETE FROM {}", self.table);

        if !self.r#where.is_empty() {
            sql.push(' ');
            sql.push_str("WHERE");
            sql.push(' ');
            for (idx, (clause, option)) in self.r#where.iter().enumerate() {
                // Separate where clauses with AND if this is not the first
                if idx != 0 {
                    sql.push(' ');
                    sql.push_str("AND");
                    sql.push(' ');
                }

                // Using :w_ prefix for parameters like in UpdateStatement
                if let Some(like) = option {
                    let _ = write!(sql, "{clause} {like} :w_{clause}");
                } else {
                    let _ = write!(sql, "{clause} = :w_{clause}");
                }
            }
        }
        // Note: A DELETE statement without a WHERE clause is valid (deletes all rows),
        // but potentially dangerous. We allow it here, but usage should be cautious.
        sql
    }
}

impl<'a> DeleteStatement<'a> {
    /// Construct a new [`DeleteStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify the table to delete from.
    pub fn table(&mut self, table_name: &'a str) -> &mut Self {
        self.table = table_name;
        self
    }

    /// Specify a clause to add to the WHERE section of the query.
    ///
    /// NOTE: where parameters are added with the `:w_${clause}` name
    /// to ensure they are distinct from any potential future parameter needs.
    pub fn r#where(&mut self, clause: &'a str) -> &mut Self {
        self.r#where.push((clause, None));
        self
    }

    // Optionally, add a 'like' method if needed, similar to SelectStatement
    // pub fn like(&mut self, clause: &'a str) -> &mut Self {
    //     self.r#where.push((clause, Some("LIKE")));
    //     self
    // }
}

#[cfg(test)]
mod tests {
    use crate::database::sqlite::{Query, query::SqlString};

    #[test]
    fn test_to_sql_simple_delete() {
        let query = Query::delete().table("history").r#where("id").to_owned();

        assert_eq!(
            query.to_sql(),
            String::from("DELETE FROM history WHERE id = :w_id")
        );
    }

    #[test]
    fn test_to_sql_multi_where() {
        let query = Query::delete()
            .table("logs")
            .r#where("level")
            .r#where("timestamp")
            .to_owned();

        assert_eq!(
            query.to_sql(),
            String::from("DELETE FROM logs WHERE level = :w_level AND timestamp = :w_timestamp")
        );
    }

    #[test]
    fn test_to_sql_delete_all() {
        // Test deleting without a where clause (use with caution!)
        let query = Query::delete().table("temp_data").to_owned();

        assert_eq!(query.to_sql(), String::from("DELETE FROM temp_data"));
    }
}
