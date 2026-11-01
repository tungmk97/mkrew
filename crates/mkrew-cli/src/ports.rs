//! `mk ports`: every listening TCP port of the daemon's machine, and `mk ports stop`
//! (spec 2026-11-01, SPEC §9).

use std::fmt::Write as _;

use anyhow::bail;
use mkrew_protocol::{PortRow, PortStopParams, Project, Request};
use ulid::Ulid;

use crate::client::Client;

pub fn list(client: &mut Client, json: bool) -> anyhow::Result<()> {
    let rows: Vec<PortRow> = client.call(Request::PortList)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no ports are listening");
        return Ok(());
    }
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let names: Vec<(Ulid, String)> = projects.into_iter().map(|p| (p.id, p.name)).collect();
    print!("{}", table(&rows, &names));
    Ok(())
}

pub fn stop(client: &mut Client, port: u16, pid: Option<u32>) -> anyhow::Result<()> {
    let pid = match pid {
        Some(pid) => pid,
        None => holder(&client.call::<Vec<PortRow>>(Request::PortList)?, port)?,
    };
    let () = client.call(Request::PortStop(PortStopParams { port, pid }))?;
    println!("stopped pid {pid} on :{port}");
    Ok(())
}

/// The one pid listening on `port`.
fn holder(rows: &[PortRow], port: u16) -> anyhow::Result<u32> {
    let mut pids: Vec<u32> = rows
        .iter()
        .filter(|r| r.port == port)
        .map(|r| r.pid)
        .collect();
    pids.sort_unstable();
    pids.dedup();
    match pids.as_slice() {
        [] => bail!("nothing listens on :{port}"),
        [pid] => Ok(*pid),
        many => {
            let list: Vec<String> = many.iter().map(u32::to_string).collect();
            bail!(
                "several processes listen on :{port} (pids {}); pick one with --pid",
                list.join(", ")
            )
        }
    }
}

/// `PORT  WORKSPACE  WHERE  SOURCE  PID`; every column but the last padded to its widest
/// cell; `-` for none.
fn table(rows: &[PortRow], projects: &[(Ulid, String)]) -> String {
    let header = ["PORT", "WORKSPACE", "WHERE", "SOURCE", "PID"].map(str::to_owned);
    let cells: Vec<[String; 5]> = rows
        .iter()
        .map(|r| {
            let workspace = r
                .project_id
                .and_then(|id| projects.iter().find(|(p, _)| *p == id))
                .map_or_else(|| "-".to_owned(), |(_, name)| name.clone());
            let place = match (&r.branch, r.project_id) {
                (Some(branch), _) => branch.clone(),
                (None, Some(_)) => "main".to_owned(),
                (None, None) => "-".to_owned(),
            };
            let source = r
                .run_command
                .clone()
                .or_else(|| r.session_title.clone())
                .unwrap_or_else(|| r.process.clone());
            [
                format!(":{}", r.port),
                workspace,
                place,
                source,
                r.pid.to_string(),
            ]
        })
        .collect();
    let all: Vec<&[String; 5]> = std::iter::once(&header).chain(&cells).collect();
    let width = |i: usize| all.iter().map(|c| c[i].chars().count()).max().unwrap_or(0);
    let (w0, w1, w2, w3) = (width(0), width(1), width(2), width(3));
    let mut out = String::new();
    for c in all {
        // Writing to a String cannot fail.
        let _ = writeln!(
            out,
            "{:<w0$}  {:<w1$}  {:<w2$}  {:<w3$}  {}",
            c[0], c[1], c[2], c[3], c[4]
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(port: u16, pid: u32) -> PortRow {
        PortRow {
            port,
            addr: "0.0.0.0".into(),
            pid,
            process: "node".into(),
            project_id: None,
            worktree_task_id: None,
            branch: None,
            session_id: None,
            session_title: None,
            run_command: None,
        }
    }

    #[test]
    fn the_holder_is_the_one_pid_on_the_port() {
        assert_eq!(holder(&[row(3000, 7), row(4000, 8)], 3000).unwrap(), 7);
        assert_eq!(
            holder(&[row(4000, 8)], 3000).unwrap_err().to_string(),
            "nothing listens on :3000"
        );
        assert_eq!(
            holder(&[row(3000, 9), row(3000, 7)], 3000)
                .unwrap_err()
                .to_string(),
            "several processes listen on :3000 (pids 7, 9); pick one with --pid"
        );
    }

    #[test]
    fn the_table_names_workspace_and_place() {
        let id = Ulid(1);
        let mut in_worktree = row(5173, 12);
        in_worktree.project_id = Some(id);
        in_worktree.branch = Some("fix".into());
        in_worktree.run_command = Some("web".into());
        assert_eq!(
            table(&[in_worktree, row(5432, 40)], &[(id, "app".to_owned())]),
            "PORT   WORKSPACE  WHERE  SOURCE  PID\n\
             :5173  app        fix    web     12\n\
             :5432  -          -      node    40\n"
        );
    }
}
