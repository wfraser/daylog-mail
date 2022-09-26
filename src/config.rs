use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Config {
    #[serde(rename = "database")]
    pub database_path: PathBuf,

    #[serde(rename = "secret_key")]
    pub secret_key_path: PathBuf,

    pub return_addr: String,

    #[serde(with = "serde_yaml::with::singleton_map")] // instead of YAML '!tag' syntax
    pub incoming_mail: IncomingMailConfig,
}

impl Config {
    pub fn try_from_arg(os_str: &OsStr) -> Result<Self, String> {
        let config_path = std::fs::canonicalize(&Path::new(os_str))
            .map_err(|e| format!("Unable to canonicalize path {:?}: {}", os_str, e))?;
        let file = File::open(&config_path)
            .map_err(|e| format!("Error opening config file {:?}: {}", config_path, e))?;
        let mut config: Self = serde_yaml::from_reader(file)
            .map_err(|e| format!("Error parsing config file {:?}: {}", config_path, e))?;
        config.resolve_paths(config_path.parent().unwrap());
        Ok(config)
    }

    pub fn resolve_paths(&mut self, base_path: &Path) {
        for path_mut in &mut [&mut self.database_path, &mut self.secret_key_path] {
            Self::resolve_path(path_mut, base_path);
        }
        let IncomingMailConfig::Maildir { path: ref mut incoming_path } = &mut self.incoming_mail;
        Self::resolve_path(incoming_path, base_path);
    }

    fn resolve_path(path: &mut PathBuf, base_path: &Path) {
        if !path.is_absolute() {
            *path = base_path.join(&path);
        }
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum IncomingMailConfig {
    /// Maildir path
    #[serde(rename = "maildir")]
    Maildir {
        path: PathBuf,
    },

    // and maybe other sources in the future?
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_deserialize() {
        let yaml = r"
database: /some/db.sqlite
secret_key: /some/secret/file
return_addr: daylog@example.com
incoming_mail:
    maildir:
        path: /var/spool/mail/daylog
";
        let deserialized: Config = serde_yaml::from_str(yaml).expect("failed to deserialize");
        let expected = Config {
            database_path: PathBuf::from("/some/db.sqlite"),
            secret_key_path: PathBuf::from("/some/secret/file"),
            return_addr: "daylog@example.com".to_owned(),
            incoming_mail: IncomingMailConfig::Maildir {
                path: PathBuf::from("/var/spool/mail/daylog"),
            },
        };
        assert_eq!(deserialized, expected);
    }
}
