use super::SqlString;

/// Update existing rows in the table
#[derive(Default, Debug, Clone)]
pub struct UpdateStatement<'a> {
    pub columns: Vec<&'a str>,
    pub table: &'a str,
    pub r#where: Vec<(&'a str, Option<&'a str>)>,
}

impl SqlString for UpdateStatement<'_> {
    /// Convert the [`UpdateStatement`] into a runnable SQL string.
    fn to_sql(&self) -> String {
        let mut sql = String::new();
        sql.push_str(&format!("UPDATE {} SET", self.table));

        for (idx, &col) in self.columns.iter().enumerate() {
            sql.push(' ');
            sql.push_str(&format!("{col} = :{col}"));

            if idx < self.columns.len() - 1 {
                sql.push(',');
            }
        }

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
                    sql.push_str(&format!("{clause} {like} :w_{clause}"));
                } else {
                    sql.push_str(&format!("{clause} = :w_{clause}"));
                }
            }
        }
        sql
    }
}

impl<'a> UpdateStatement<'a> {

    /// Construct a new [`UpdateStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify the table of the update.
    pub fn table(&mut self, table_name: &'a str) -> &mut Self {
        self.table = table_name;
        self
    }

    /// Specify a clause to add to the WHERE section of the query.
    ///
    /// NOTE: where parameters are added with the `:w_${clause}` name
    /// so that they do not conflict with the column values
    pub fn r#where(&mut self, clause: &'a str) -> &mut Self {
        self.r#where.push((clause, None));
        self
    }

    /// Specify a column to update.
    pub fn column(&mut self, column: &'a str) -> &mut Self {
        self.columns.push(column);
        self
    }
}

#[test]
fn test_to_sql() {
    let query = super::Query::update()
        .table("history")
        .column("command")
        .column("cwd")
        .r#where("id")
        .to_owned();

    assert_eq!(
        query.to_sql(),
        String::from(concat!(
            "UPDATE history SET command = :command, cwd = :cwd WHERE id = :w_id",
        ))
    );
}
