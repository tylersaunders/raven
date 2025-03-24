use super::SqlString;


/// Select rows from an existing table
#[derive(Default, Debug, Clone)]
pub struct SelectStatement<'a> {
    pub from: Vec<&'a str>,
    pub selects: Vec<SelectExpr>,
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

                if let Some(like) = option {
                    sql.push_str(&format!("{clause} {like} :{clause}"));
                } else {
                    sql.push_str(&format!("{clause} = :{clause}"));
                }
            }
        }

        if let Some((column, direction)) = &self.orderby {
            sql.push(' ');
            sql.push_str(&format!("ORDER BY {column} {direction}"));
        }

        if let Some(limit) = self.limit {
            sql.push(' ');
            sql.push_str(&format!("LIMIT {limit}"));
            sql.push(' ');
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

    /// Specify the table to select results from.
    pub fn from(&mut self, table_name: &'a str) -> &mut Self {
        self.from.push(table_name);
        self
    }

    /// Specify a clause to add to the WHERE section of the query.
    /// NOTE: parameters are added with the name `":${clause}"`
    pub fn r#where(&mut self, clause: &'a str) -> &mut Self {
        self.r#where.push((clause, None));
        self
    }

    /// Specify a LIKE clause to add to the WHERE section of the query.
    /// NOTE: parameters are added with the name `":${clause}"`
    pub fn like(&mut self, clause: &'a str) -> &mut Self {
        self.r#where.push((clause, Some("LIKE")));
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

#[test]
fn test_to_sql() {
    let query = super::Query::select()
        .column("id")
        .column("command")
        .column("cwd")
        .column("exit_code")
        .column("timestamp")
        .count("id", "count_id")
        .from("history")
        .r#where("id")
        .r#where("test")
        .r#where("two")
        .like("command")
        .limit(100)
        .orderby("timestamp", "DESC")
        .to_owned();

    assert_eq!(
        query.to_sql(),
        String::from(concat!(
            "SELECT id, command, cwd, exit_code, timestamp, count(id) AS count_id FROM history ",
            "WHERE id = :id AND test = :test AND two = :two AND command LIKE :command ",
            "ORDER BY timestamp DESC LIMIT 100 ",
        ))
    );



}
