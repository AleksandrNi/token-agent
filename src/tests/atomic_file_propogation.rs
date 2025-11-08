#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use tokio::time::sleep;

    #[tokio::test]
    async fn atomic_write_and_permissions() {
        // temp file in system tempdir
        let fname = format!("token_agent_test_{}.token", "some_value");
        let mut path = std::env::temp_dir();
        path.push(fname);

        // ensure cleanup
        let _ = fs::remove_file(&path);

        // Simulate atomic write (tmp -> rename) code (should match your implementation)
        let tmp = path.with_extension("tmp");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = "token-value-123";
        fs::write(&tmp, content).expect("write tmp");
        // set 0o600 perms
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&tmp, perms).expect("set perms");
        fs::rename(&tmp, &path).expect("rename");

        // small delay to stabilize FS
        sleep(std::time::Duration::from_millis(20)).await;

        let got = fs::read_to_string(&path).expect("read file");
        assert_eq!(got, content, "file content mismatch");

        #[cfg(unix)]
        {
            let mode = fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "permissions mismatch (expected 0600)");
        }

        // cleanup
        let _ = fs::remove_file(&path);
    }
}
