use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const MAX_BACKOFF_MS: u64 = 5000;
const INITIAL_BACKOFF_MS: u64 = 10;

pub struct Lock {
    lock_path: PathBuf,
    _pid: u32,
}

impl Lock {
    /// Acquire a coarse-grained lock on the beads directory
    pub fn acquire(beads_dir: &Path) -> Result<Self> {
        let lock_path = beads_dir.join("minibeads.lock");
        let pid = std::process::id();

        let mut backoff = INITIAL_BACKOFF_MS;
        let mut total_wait = 0;

        loop {
            // Try to create lock file
            match try_acquire_lock(&lock_path, pid) {
                Ok(()) => {
                    return Ok(Self { lock_path, _pid: pid });
                }
                Err(e) => {
                    // Check if we've exceeded max backoff time
                    if total_wait >= MAX_BACKOFF_MS {
                        anyhow::bail!(
                            "Failed to acquire lock after {}ms: {}",
                            MAX_BACKOFF_MS,
                            e
                        );
                    }

                    // Wait with exponential backoff
                    thread::sleep(Duration::from_millis(backoff));
                    total_wait += backoff;
                    backoff = (backoff * 2).min(MAX_BACKOFF_MS - total_wait);
                }
            }
        }
    }

    /// Release the lock
    #[allow(dead_code)]
    pub fn release(self) -> Result<()> {
        // Lock is released in Drop, but we provide explicit method too
        drop(self);
        Ok(())
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // Try to remove lock file, but don't panic if it fails
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn try_acquire_lock(lock_path: &Path, pid: u32) -> Result<()> {
    // Check if lock file exists
    if lock_path.exists() {
        // Read the PID from the lock file
        let content = fs::read_to_string(lock_path).context("Failed to read lock file")?;
        if let Ok(existing_pid) = content.trim().parse::<u32>() {
            // Check if the process is still alive
            if is_process_alive(existing_pid) {
                anyhow::bail!("Lock held by process {}", existing_pid);
            } else {
                // Stale lock - remove it
                fs::remove_file(lock_path).context("Failed to remove stale lock")?;
            }
        } else {
            // Invalid lock file - remove it
            fs::remove_file(lock_path).context("Failed to remove invalid lock")?;
        }
    }

    // Create lock file with our PID
    fs::write(lock_path, pid.to_string()).context("Failed to write lock file")?;

    Ok(())
}

#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    use std::io;

    // Try to send signal 0 (does not kill, just checks if process exists)
    let result = unsafe { libc::kill(pid as i32, 0) };

    if result == 0 {
        true
    } else {
        let error = io::Error::last_os_error();
        // ESRCH means process doesn't exist
        error.raw_os_error() != Some(libc::ESRCH)
    }
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use std::ptr;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            false
        } else {
            CloseHandle(handle);
            true
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn is_process_alive(_pid: u32) -> bool {
    // Conservative: assume process is alive if we can't check
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_lock_acquire_release() {
        let temp_dir = env::temp_dir().join(format!("beads_test_{}", std::process::id()));
        fs::create_dir_all(&temp_dir).unwrap();

        let lock = Lock::acquire(&temp_dir).unwrap();
        assert!(temp_dir.join("minibeads.lock").exists());

        drop(lock);
        assert!(!temp_dir.join("minibeads.lock").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
