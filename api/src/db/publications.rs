use std::{borrow::Cow, collections::HashMap};

use serde::Serialize;
use sqlx::{postgres::PgConnectOptions, Connection, Executor, PgConnection, Row};
use tracing::error;

use super::tables::Table;

pub fn quote_identifier(identifier: &str) -> Cow<'_, str> {
    if identifier.find('"').is_some() {
        Cow::Owned(quote_identifier_alloc(identifier))
    } else {
        Cow::Borrowed(identifier)
    }
}

fn quote_identifier_alloc(identifier: &str) -> String {
    let mut quoted_identifier = String::with_capacity(identifier.len());
    for char in identifier.chars() {
        if char == '"' {
            quoted_identifier.push('"');
        }
        quoted_identifier.push(char);
    }
    quoted_identifier
}

pub fn quote_literal(literal: &str) -> String {
    let mut quoted_literal = String::with_capacity(literal.len() + 2);

    if literal.find('\\').is_some() {
        quoted_literal.push('E');
    }

    quoted_literal.push('\'');

    for char in literal.chars() {
        if char == '\'' {
            quoted_literal.push('\'');
        } else if char == '\\' {
            quoted_literal.push('\\');
        }

        quoted_literal.push(char);
    }

    quoted_literal.push('\'');

    quoted_literal
}

#[derive(Serialize)]
pub struct Publication {
    pub name: String,
    pub tables: Vec<Table>,
}

pub async fn create_publication(
    publication: &Publication,
    options: &PgConnectOptions,
) -> Result<(), sqlx::Error> {
    let mut query = String::new();
    let quoted_publication_name = quote_identifier(&publication.name);
    query.push_str("create publication ");
    query.push_str(&quoted_publication_name);
    query.push_str(" for table only ");

    for (i, table) in publication.tables.iter().enumerate() {
        let quoted_schema = quote_identifier(&table.schema);
        let quoted_name = quote_identifier(&table.name);
        query.push_str(&quoted_schema);
        query.push('.');
        query.push_str(&quoted_name);

        if i < publication.tables.len() - 1 {
            query.push(',')
        }
    }

    let mut connection = PgConnection::connect_with(options).await?;
    connection.execute(query.as_str()).await?;

    Ok(())
}

pub async fn update_publication(
    publication: &Publication,
    options: &PgConnectOptions,
) -> Result<(), sqlx::Error> {
    let mut query = String::new();
    let quoted_publication_name = quote_identifier(&publication.name);
    query.push_str("alter publication ");
    query.push_str(&quoted_publication_name);
    query.push_str(" set table only ");

    for (i, table) in publication.tables.iter().enumerate() {
        let quoted_schema = quote_identifier(&table.schema);
        let quoted_name = quote_identifier(&table.name);
        query.push_str(&quoted_schema);
        query.push('.');
        query.push_str(&quoted_name);

        if i < publication.tables.len() - 1 {
            query.push(',')
        }
    }

    let mut connection = PgConnection::connect_with(options).await?;
    connection.execute(query.as_str()).await?;

    Ok(())
}

pub async fn drop_publication(
    publication_name: &str,
    options: &PgConnectOptions,
) -> Result<(), sqlx::Error> {
    let mut query = String::new();
    query.push_str("drop publication if exists ");
    let quoted_publication_name = quote_identifier(publication_name);
    query.push_str(&quoted_publication_name);

    let mut connection = PgConnection::connect_with(options).await?;
    connection.execute(query.as_str()).await?;

    Ok(())
}

pub async fn read_publication(
    publication_name: &str,
    options: &PgConnectOptions,
) -> Result<Option<Publication>, sqlx::Error> {
    let mut query = String::new();
    query.push_str(
        r#"
        select p.pubname, pt.schemaname, pt.tablename from pg_publication p
        join pg_publication_tables pt on p.pubname = pt.pubname
        where
           	p.puballtables = false
           	and p.pubinsert = true
           	and p.pubupdate = true
           	and p.pubdelete = true
           	and p.pubtruncate = true
            and p.pubname =
	   "#,
    );

    let quoted_publication_name = quote_literal(publication_name);
    query.push_str(&quoted_publication_name);

    error!("QUERY: {query}");

    let mut connection = PgConnection::connect_with(options).await?;

    let mut tables = vec![];
    let mut name: Option<String> = None;

    for row in connection.fetch_all(query.as_str()).await? {
        let pub_name: String = row.get("pubname");
        if let Some(ref name) = name {
            assert_eq!(name.as_str(), pub_name);
        } else {
            name = Some(pub_name);
        }
        let schema = row.get("schemaname");
        let name = row.get("tablename");
        tables.push(Table { schema, name });
    }

    let publication = name.map(|name| Publication { name, tables });

    Ok(publication)
}

pub async fn read_all_publications(
    options: &PgConnectOptions,
) -> Result<Vec<Publication>, sqlx::Error> {
    let query = r#"
        select p.pubname, pt.schemaname, pt.tablename from pg_publication p
        join pg_publication_tables pt on p.pubname = pt.pubname
        where
           	p.puballtables = false
           	and p.pubinsert = true
           	and p.pubupdate = true
           	and p.pubdelete = true
           	and p.pubtruncate = true;
	   "#;

    let mut connection = PgConnection::connect_with(options).await?;

    let mut pub_name_to_tables: HashMap<String, Vec<Table>> = HashMap::new();

    for row in connection.fetch_all(query).await? {
        let pub_name: String = row.get("pubname");
        let schema = row.get("schemaname");
        let name = row.get("tablename");
        let tables = pub_name_to_tables.entry(pub_name).or_default();
        tables.push(Table { schema, name });
    }

    let publications = pub_name_to_tables
        .into_iter()
        .map(|(name, tables)| Publication { name, tables })
        .collect();

    Ok(publications)
}