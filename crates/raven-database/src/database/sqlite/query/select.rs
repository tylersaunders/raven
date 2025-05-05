use super::SqlString;
use std::fmt::Write as _;

/// Select rows from an existing table
#[derive(Default, Debug, Clone)]
pub struct SelectStatement<'a> {
    pub from: Vec<&'a str>,
    pub selects: Vec<SelectExpr>,
    // Tuple: (clause, operator_or_none)
    // None => "=", Some("LIKE") => "LIKE", Some("MATCH") => "MATCH"
    pub r#where: Vec<(&'a str, Option<&'a str>)>,
    pub limit: Option<usize>,
    pub orderby: Option<(&'a str, &'a str)>,
}

#[derive(Debug, Clone)]
pub struct SelectExpr {
    pub expr: String,
    pub alias: Option<String>,
}

impl SqlString for SelectStatement<'_> {
    /// Convert the [`SelectStatement`] into a runnable SQL string.
    fn to_sql(&self) -> String {
        let mut sql = String::new();
        sql.push_str("SELECT ");
        sql.push_str(
            &self
                .selects
                .iter()
                .map(|select| {
                    if let Some(alias) = &select.alias {
                        format!("{} AS {}", select.expr, alias)
                    } else {
                        select.expr.to_string()
                    }
                })
                .collect::<Vec<String>>()
                .join(", "),
        );
        sql.push_str(" FROM ");
        sql.push_str(&self.from.join(", "));

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

                // Create a valid parameter name by replacing '.' with '_'
                let param_name = clause.replace('.', "_");

                match option {
                    Some(op) => {
                        // Handle other potential future operators if needed, using clause name for parameter
                        let _ = write!(sql, "{clause} {op} :{param_name}");
                    }
                    None => {
                        // Default to equals, using clause name for parameter
                        let _ = write!(sql, "{clause} = :{param_name}");
                    }
                }
            }
        }

        if let Some((column, direction)) = &self.orderby {
            sql.push(' ');
            let _ = write!(sql, "ORDER BY {column} {direction}");
        }

        if let Some(limit) = self.limit {
            sql.push(' ');
            let _ = write!(sql, "LIMIT {limit}");
            sql.push(' '); // Add trailing space consistent with previous test
        }

        sql
    }
}

impl<'a> SelectStatement<'a> {
    /// Construct a new [`SelectStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a `COUNT(column_name)` AS alias in the selected fields.
    ///
    /// * `column_name`: The name of the column to count. Can be "*" to count all rows.
    /// * `alias`: The alias for the column in the returned rows.
    pub fn count(&mut self, column_name: &'a str, alias: &'a str) -> &mut Self {
        self.selects.push(SelectExpr {
            expr: format!("count({column_name})"),
            alias: Some(alias.to_string()),
        });
        self
    }

    /// Specify the table to select results from. Can be called multiple times for joins
    /// or provide a single string with JOIN syntax.
    pub fn from(&mut self, table_name: &'a str) -> &mut Self {
        self.from.push(table_name);
        self
    }

    /// Specify a clause to add to the WHERE section of the query using `=`.
    /// NOTE: parameters are added with the name `":{clause}"`
    pub fn r#where(&mut self, clause: &'a str) -> &mut Self {
        self.r#where.push((clause, None));
        self
    }

    /// Specify an FTS5 MATCH clause to add to the WHERE section.
    /// The `match_clause` should typically be the name/alias of the FTS table.
    /// NOTE: parameters are added with the name `":{match_clause}"`
    pub fn match_fts(&mut self, match_clause: &'a str) -> &mut Self {
        self.r#where.push((match_clause, Some("MATCH")));
        self
    }

    /// Specify which column to add to the selection list.
    pub fn column(&mut self, column: &'a str) -> &mut Self {
        self.selects.push(SelectExpr {
            expr: column.to_string(),
            alias: None,
        });
        self
    }

    /// Specify a limit on the maximum number of rows returned.
    pub fn limit(&mut self, limit: usize) -> &mut Self {
        self.limit = Some(limit);
        self
    }

    /// Specify an ORDER BY clause to order the results in the provided direction.
    pub fn orderby(&mut self, column: &'a str, direction: &'a str) -> &mut Self {
        self.orderby = Some((column, direction));
        self
    }
}

#[cfg(test)]
mod tests {
    use crate::database::sqlite::{Query, query::SqlString};

    #[test]
    fn test_to_sql_fts() {
        let query = Query::select()
            .column("h.id") // Example using alias
            .column("h.command")
            .column("h.cwd")
            .column("h.exit_code")
            .column("h.timestamp")
            .count("h.id", "count_id") // Example using alias
            .from("history h JOIN history_fts fts ON h.rowid = fts.rowid") // Example JOIN
            .r#where("h.exit_code") // Example using alias for regular where
            .r#where("fts.rank") // Example filtering on FTS internal column (requires alias)
            .match_fts("history_fts") // FTS Match clause, param :history_fts
            .limit(100)
            .orderby("rank", "DESC") // Order by rank for FTS
            .to_owned();

        // Note: Parameter names directly match the clause string provided to where/like/match_fts
        assert_eq!(
            query.to_sql(),
            String::from(concat!(
                "SELECT h.id, h.command, h.cwd, h.exit_code, h.timestamp, count(h.id) AS count_id ",
                "FROM history h JOIN history_fts fts ON h.rowid = fts.rowid ",
                "WHERE h.exit_code = :h_exit_code AND fts.rank = :fts_rank AND history_fts MATCH :history_fts ",
                "ORDER BY rank DESC LIMIT 100 ",
            ))
        );
    }

    // Add a separate test for the non-FTS case if needed
    #[test]
    fn test_to_sql_no_fts() {
        let query = Query::select()
            .column("id")
            .column("command")
            .from("history")
            .r#where("id")
            .limit(50)
            .orderby("timestamp", "ASC")
            .to_owned();

        assert_eq!(
            query.to_sql(),
            String::from(concat!(
                "SELECT id, command FROM history ",
                "WHERE id = :id ", // params :id, :command
                "ORDER BY timestamp ASC LIMIT 50 ",
            ))
        );
    }
}
