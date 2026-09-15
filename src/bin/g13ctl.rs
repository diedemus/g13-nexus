use anyhow::Result;
use g13_nexus::ipc::{self,Request};
use std::{io::{BufRead,BufReader,Write},os::unix::net::UnixStream};
fn main()->Result<()> { let arg=std::env::args().nth(1).unwrap_or("status".into()); let req=if arg=="reload"{Request::Reload}else{Request::Status}; let mut s=UnixStream::connect(ipc::socket_path())?; writeln!(s,"{}",serde_json::to_string(&req)?)?; let mut out=String::new();BufReader::new(s).read_line(&mut out)?;print!("{out}");Ok(()) }
