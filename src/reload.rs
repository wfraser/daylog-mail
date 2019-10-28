use crate::config::Config;
use crate::named_pipe::NamedPipe;
use failure::ResultExt;
use std::io::{Read, Write};

pub fn reload(config: &Config) -> Result<(), failure::Error> {
    let control = NamedPipe::open(&config.control_path)
        .with_context(|e| format!("failed to open control file {:?}: {}", config.control_path, e))?;

    /*
    control.set_nonblocking(true)
        .context("failed to set control file to nonblocking mode")?;
    */

    control.as_ref().write_all(b"?")?;

    let mut response = [0u8; 1];
    control.as_ref().read_exact(&mut response)?;

    Ok(())
}
