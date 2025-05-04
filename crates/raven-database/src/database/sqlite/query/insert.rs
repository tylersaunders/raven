use super::SqlString;

/// Insert any new rows into an existing table
#[derive(Default, Debug, Clone)]
pub struct InsertStatement<'a> {
    pub columns: Vec<&'a str>,
    pub table: &'a str,
}

impl SqlString for InsertStatement<'_> {
    fn to_sql(&self) -> String {
        let mut sql = String::new();
        sql.push_str(&format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            self.columns.join(", "),
            self.columns
                .iter()
                .map(|col| format!(":{col}"))
                .collect::<Vec<String>>()
                .join(", ")
        ));
        sql
    }
}

impl<'a> InsertStatement<'a> {
    /// Construct a new [`InsertStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify which table to insert into.
    pub fn table(&mut self, table_name: &'a str) -> &mut Self {
        self.table = table_name;
        self
    }

    /// Specify which column to add to the insertion list.
    pub fn column(&mut self, column: &'a str) -> &mut Self {
        self.columns.push(column);
        self
    }
}

#[test]
fn test_to_sql() {
    let query = super::Query::insert()
        .table("history")
        .column("command")
        .column("cwd")
        .to_owned();

    assert_eq!(
        query.to_sql(),
        String::from(concat!(
            "INSERT INTO history (command, cwd) VALUES (:command, :cwd)"
        ))
    );
}
