//! `stdin_messages` table: a FIFO queue per service.
//!
//! Ported from `src/database/stdinMessagesTable.ts`. Unlike the Node version,
//! `pop_stdin_message` wraps the select+delete in a transaction so it is atomic
//! under concurrent access (an intentional improvement noted in the porting spec).

use rusqlite::{params, Connection};

/// A row from the `stdin_messages` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StdinMessage {
    pub id: i64,
    pub command_name: String,
    pub project_dir: String,
    pub data: String,
    pub encoding: String,
    pub created_at: i64,
}

const SELECT_COLS: &str = "id, command_name, project_dir, data, encoding, created_at";

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<StdinMessage> {
    Ok(StdinMessage {
        id: row.get(0)?,
        command_name: row.get(1)?,
        project_dir: row.get(2)?,
        data: row.get(3)?,
        encoding: row.get(4)?,
        created_at: row.get(5)?,
    })
}

/// Insert a new stdin message. `encoding` defaults to `"utf8"` when `None`.
/// Returns the new row id.
pub fn create_stdin_message(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
    data: &str,
    encoding: Option<&str>,
) -> rusqlite::Result<i64> {
    conn.execute(
        "insert into stdin_messages (command_name, project_dir, data, encoding) values (?1, ?2, ?3, ?4)",
        params![command_name, project_dir, data, encoding.unwrap_or("utf8")],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Pop the oldest (lowest id) pending message for a service, deleting it.
/// Returns `None` if the queue is empty. The select+delete run in a transaction.
pub fn pop_stdin_message(
    conn: &mut Connection,
    command_name: &str,
    project_dir: &str,
) -> rusqlite::Result<Option<StdinMessage>> {
    let tx = conn.transaction()?;

    let message = {
        let mut stmt = tx.prepare(&format!(
            "select {SELECT_COLS} from stdin_messages where command_name = ?1 and project_dir = ?2 order by id asc limit 1"
        ))?;
        let mut rows = stmt.query_map(params![command_name, project_dir], row_to_message)?;
        match rows.next() {
            Some(row) => Some(row?),
            None => None,
        }
    };

    if let Some(ref msg) = message {
        tx.execute("delete from stdin_messages where id = ?1", params![msg.id])?;
    }

    tx.commit()?;
    Ok(message)
}

/// Delete all pending messages for a service.
pub fn clear_stdin_messages(
    conn: &Connection,
    command_name: &str,
    project_dir: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "delete from stdin_messages where command_name = ?1 and project_dir = ?2",
        params![command_name, project_dir],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{get_database, temp_db_dir};

    #[test]
    fn fifo_pop_and_clear() {
        let dir = temp_db_dir("stdin");
        let mut conn = get_database(Some(&dir)).unwrap();

        create_stdin_message(&conn, "api", "/proj", "first", None).unwrap();
        create_stdin_message(&conn, "api", "/proj", "second", Some("utf8")).unwrap();
        create_stdin_message(&conn, "api", "/proj", "third", None).unwrap();

        // FIFO: oldest id first.
        let m1 = pop_stdin_message(&mut conn, "api", "/proj").unwrap().unwrap();
        assert_eq!(m1.data, "first");
        assert_eq!(m1.encoding, "utf8");
        let m2 = pop_stdin_message(&mut conn, "api", "/proj").unwrap().unwrap();
        assert_eq!(m2.data, "second");

        // One left; clear empties it.
        clear_stdin_messages(&conn, "api", "/proj").unwrap();
        let after = pop_stdin_message(&mut conn, "api", "/proj").unwrap();
        assert!(after.is_none());

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pop_scopes_to_service() {
        let dir = temp_db_dir("stdin-scope");
        let mut conn = get_database(Some(&dir)).unwrap();

        create_stdin_message(&conn, "api", "/proj", "for-api", None).unwrap();
        create_stdin_message(&conn, "worker", "/proj", "for-worker", None).unwrap();

        let popped = pop_stdin_message(&mut conn, "worker", "/proj").unwrap().unwrap();
        assert_eq!(popped.data, "for-worker");

        // api message still present.
        let api = pop_stdin_message(&mut conn, "api", "/proj").unwrap().unwrap();
        assert_eq!(api.data, "for-api");

        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
