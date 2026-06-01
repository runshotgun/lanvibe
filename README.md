# LANVibe

A small cross-platform utility for finding HTTP and HTTPS web interfaces on selected LAN devices.

## What it does

- Discovers LAN devices from ARP plus a lightweight local subnet ping sweep.
- Lets the user choose which devices are scannable.
- Scans all TCP ports on selected devices for HTTP/HTTPS services.
- Keeps inactive services visible for a configurable retention window, defaulting to 30 days.
- Shows a Tauri tray/status popup on desktop.
- Hosts the same dashboard on the LAN, defaulting to `http://<lan-ip>:8765`.

## Development

```bash
npm install
npm run build
npm run tauri -- dev
```

This project requires the Rust toolchain for Tauri builds. Install it from <https://rustup.rs/>.

## Releases and updates

LANVibe uses Tauri's signed updater with GitHub Releases. Release artifacts are built by
`.github/workflows/release.yml` when a tag matching `app-v*` is pushed.

Before the first release, add the updater signing secrets to the GitHub repository:

```powershell
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo runshotgun/lanvibe --body (Get-Content -Raw "$env:USERPROFILE\.tauri\lanvibe-updater.key")
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo runshotgun/lanvibe --body (Get-Content -Raw "$env:USERPROFILE\.tauri\lanvibe-updater-key-password.txt")
```

The updater public key is committed in `src-tauri/tauri.conf.json`; the private key must stay out of git.
GitHub publishes the updater manifest at:

```text
https://github.com/runshotgun/lanvibe/releases/latest/download/latest.json
```

To release:

```powershell
npm version patch --no-git-tag-version
# Keep src-tauri/tauri.conf.json and src-tauri/Cargo.toml versions in sync.
git commit -am "chore: release v0.1.1"
git tag app-v0.1.1
git push origin main --tags
```

Windows and macOS OS-level code signing/notarization are separate from Tauri updater signing. The updater
signature proves the update came from the LANVibe release key, while OS signing reduces install warnings.

## Notes

The dashboard is intentionally open to the LAN. Only devices explicitly selected in the app are scanned across all ports.
