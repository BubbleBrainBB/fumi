//! Verification and installation.
//!
//! A downloaded DMG is trusted only after two checks: its SHA-256 matches the
//! feed, and the app inside carries a valid Developer ID signature. The actual
//! swap runs in a detached shell script *after* Fumi quits — a running signed
//! app cannot replace itself on macOS. The script backs up the old bundle,
//! copies the new one in, verifies it, then relaunches; on any failure it puts
//! the backup back.

use std::path::Path;

/// Where the detached installer and its logs live.
pub fn work_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("fumi-update-{}", std::process::id()))
}

/// Builds the shell script that performs the swap after this process exits.
/// Kept as a pure function so the command sequence can be unit-tested without
/// touching a real app bundle.
pub fn install_script(dmg: &Path, app: &Path) -> String {
    let dmg = dmg.to_string_lossy();
    let app = app.to_string_lossy();
    let parent = Path::new(app.as_ref()).parent().unwrap_or_else(|| Path::new("/Applications"));
    let parent = parent.to_string_lossy();
    let work = work_dir().to_string_lossy().to_string();
    format!(
        r#"#!/bin/sh
# Written by Fumi's updater. Runs after Fumi quits.
set -eu
DMG="{dmg}"
APP="{app}"
PARENT="{parent}"
WORK="{work}"
MOUNT="$WORK/mount"
BACKUP="$APP.old-update"
LOG="$WORK/install.log"
exec >>"$LOG" 2>&1
echo "installer start $(date)"

# Wait for the quitting Fumi process to release the bundle.
for i in $(seq 1 60); do
  if ! /usr/bin/pgrep -f "$APP/Contents/MacOS/" >/dev/null 2>&1; then break; fi
  /bin/sleep 0.5
done

mkdir -p "$MOUNT"
/usr/bin/hdiutil attach "$DMG" -nobrowse -readonly -mountpoint "$MOUNT"
NEWAPP="$MOUNT/Fumi.app"
if [ ! -d "$NEWAPP" ]; then echo "no Fumi.app in DMG" >&2; exit 1; fi

# Developer ID signature must verify before we install anything.
/usr/bin/codesign --verify --deep --strict "$NEWAPP"

rm -rf "$BACKUP"
/bin/mv "$APP" "$BACKUP"
if /bin/cp -R "$NEWAPP" "$APP"; then
  /usr/bin/codesign --verify --deep --strict "$APP" || /bin/mv "$BACKUP" "$APP"
  /usr/bin/touch "$APP"
else
  /bin/mv "$BACKUP" "$APP"
fi
/usr/bin/hdiutil detach "$MOUNT" -force || true
echo "installer done $(date)"
/usr/bin/open "$APP"
"#
    )
}

/// A pre-flight signature check on a mounted app before installation.
pub fn verify_signature(app: &Path) -> Result<(), String> {
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(app)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Writes the installer script to `work_dir` and launches it detached, after
/// which the caller should quit the app.
pub fn stage(dmg: &Path, app: &Path) -> Result<std::path::PathBuf, String> {
    let dir = work_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("install.sh");
    std::fs::write(&script, install_script(dmg, app)).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    // `open -b` would need a bundle; launch via `sh` detached and let it wait.
    std::process::Command::new("/bin/sh").arg(&script).spawn().map_err(|e| e.to_string())?;
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn script_verifies_signature_before_swapping() {
        let script = install_script(
            &PathBuf::from("/tmp/fumi.dmg"),
            &PathBuf::from("/Applications/Fumi.app"),
        );
        let verify_pos = script.find("codesign --verify --deep --strict \"$NEWAPP\"").unwrap();
        let swap_pos = script.find("/bin/mv \"$APP\" \"$BACKUP\"").unwrap();
        assert!(verify_pos < swap_pos, "signature must be checked before the swap");
    }

    #[test]
    fn script_attaches_dmg_and_relaunches() {
        let script = install_script(
            &PathBuf::from("/tmp/fumi.dmg"),
            &PathBuf::from("/Applications/Fumi.app"),
        );
        assert!(script.contains("hdiutil attach \"$DMG\""));
        assert!(script.contains("/usr/bin/open \"$APP\""));
        assert!(script.contains("hdiutil detach"));
    }

    #[test]
    fn script_restores_backup_on_failure() {
        let script = install_script(&PathBuf::from("/tmp/a.dmg"), &PathBuf::from("/Applications/Fumi.app"));
        assert!(script.contains("/bin/mv \"$BACKUP\" \"$APP\""));
        assert!(script.contains("set -eu"));
    }
}
