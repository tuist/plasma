//! Credential persistence shared by terminal and headless hosts.

use std::{
    env, fs,
    fs::OpenOptions,
    io,
    path::{Path, PathBuf},
};

use fs2::FileExt;

const CREDENTIAL_FILE: &str = "openrouter.key";
const LOCK_FILE: &str = ".credentials.lock";

/// Interface for hosts that need to supply their own credential persistence.
pub trait CredentialStore {
    fn load(&self) -> io::Result<Option<String>>;
    fn save(&self, credential: &str) -> io::Result<()>;
    fn delete(&self) -> io::Result<()>;
}

/// A lock-protected credential directory. The default follows the XDG Base
/// Directory Specification <https://specifications.freedesktop.org/basedir-spec/latest/>,
/// and `PLASMA_CREDENTIAL_DIR` lets harnesses select an isolated location.
#[derive(Clone, Debug)]
pub struct FileCredentialStore {
    directory: PathBuf,
}

impl FileCredentialStore {
    pub fn from_environment() -> io::Result<Self> {
        let directory = match env::var_os("PLASMA_CREDENTIAL_DIR") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => xdg_config_home()?.join("plasma"),
        };
        Ok(Self { directory })
    }

    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    fn credential_path(&self) -> PathBuf {
        self.directory.join(CREDENTIAL_FILE)
    }

    fn write_credential(&self, credential: &str) -> io::Result<()> {
        let path = self.credential_path();
        let temporary = self
            .directory
            .join(format!(".{CREDENTIAL_FILE}.{}.tmp", std::process::id()));
        fs::write(&temporary, credential)?;
        set_private_permissions(&temporary)?;
        fs::rename(temporary, path)
    }

    fn with_lock<T>(&self, operation: impl FnOnce() -> io::Result<T>) -> io::Result<T> {
        fs::create_dir_all(&self.directory)?;
        set_private_permissions(&self.directory)?;
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.directory.join(LOCK_FILE))?;
        set_private_permissions(&self.directory.join(LOCK_FILE))?;
        lock.lock_exclusive()?;
        let result = operation();
        let unlock_result = fs2::FileExt::unlock(&lock);
        match result {
            Ok(value) => {
                unlock_result?;
                Ok(value)
            }
            Err(error) => {
                let _ = unlock_result;
                Err(error)
            }
        }
    }
}

impl CredentialStore for FileCredentialStore {
    fn load(&self) -> io::Result<Option<String>> {
        self.with_lock(|| match fs::read_to_string(self.credential_path()) {
            Ok(credential) => Ok(Some(credential)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        })
    }

    fn save(&self, credential: &str) -> io::Result<()> {
        self.with_lock(|| self.write_credential(credential))
    }

    fn delete(&self) -> io::Result<()> {
        self.with_lock(|| match fs::remove_file(self.credential_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        })
    }
}

fn xdg_config_home() -> io::Result<PathBuf> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("could not resolve the home directory"))?;
    Ok(home.join(".config"))
}

fn set_private_permissions(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_store() -> FileCredentialStore {
        FileCredentialStore::new(
            env::temp_dir().join(format!("plasma-credential-test-{}", std::process::id())),
        )
    }

    #[test]
    fn store_round_trips_and_deletes_credentials() {
        let store = temporary_store();
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
        store.save("test credential").unwrap();
        assert_eq!(store.load().unwrap().as_deref(), Some("test credential"));
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
    }

    #[test]
    fn custom_directory_is_preserved() {
        let path = PathBuf::from("/tmp/plasma-credentials");
        assert_eq!(FileCredentialStore::new(path.clone()).directory(), path);
    }
}
