# Screen Tracker — Full Setup Guide

This guide walks you through everything needed to get the app running on Mac and Windows.
Do Steps 1–3 once (Google Cloud setup). Then do Step 4 on each machine.

---

## Step 1 — Create a Google Cloud project and service account

This gives the app a "robot" credential that can write to your Drive without needing your password.

1. Go to **https://console.cloud.google.com** and sign in with your **personal** Google account.
2. Click the project dropdown at the top → **New Project** → name it anything (e.g. "ScreenTracker") → Create.
3. In the left menu: **APIs & Services → Library**.
   - Search for **Google Drive API** → Enable it.
4. Go to **APIs & Services → Credentials → Create Credentials → Service Account**.
   - Name: `screen-tracker` → Create and Continue → Done.
5. Click the service account you just created → **Keys** tab → **Add Key → Create new key → JSON**.
   - A `.json` file downloads. **Keep this safe — treat it like a password.**
6. Copy the service account's **email address** (looks like `screen-tracker@yourproject.iam.gserviceaccount.com`).

---

## Step 2 — Create and share the Drive folder

1. In **your personal Google Drive**, create a new folder (e.g. `ScreenTracker Data`).
2. **Right-click the folder → Share → share with the service account email** from Step 1.
   - Role: **Editor**. Uncheck "Notify people". Click Share.
3. Open the folder and copy the **folder ID** from the URL:
   ```
   https://drive.google.com/drive/folders/  1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs
                                             ↑ this part is the folder ID
   ```

---

## Step 3 — Set up Gmail App Password (for report emails)

The app sends email via Gmail SMTP. You need an App Password because normal passwords don't work with SMTP when 2FA is on.

1. Make sure **2-Factor Authentication** is enabled on your Gmail account.
   (Google Account → Security → 2-Step Verification)
2. Go to: **Google Account → Security → App passwords** (search "App passwords" if you don't see it).
3. Select app: **Mail**, device: **Other** → type "ScreenTracker" → Generate.
4. Copy the **16-character password** shown. You won't see it again (but you can generate a new one any time).

---

## Step 4 — Install and configure the app on each machine

### Mac

```bash
# 1. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 2. Install Node.js (needed for Tauri CLI only — not bundled in final app)
#    Download from https://nodejs.org or via brew:
brew install node

# 3. Install Tauri CLI
cargo install tauri-cli --version "^1.6"

# 4. Build the app (from the project root directory)
cd /path/to/screen-tracker
cargo tauri build

# 5. The built app is at:
#    src-tauri/target/release/bundle/macos/Screen Tracker.app
#    Drag it to /Applications and double-click to launch.
```

**Grant permissions when prompted:**
- Accessibility access (needed to detect active app via osascript)
- Go to: System Settings → Privacy & Security → Accessibility → add Screen Tracker

### Windows

```powershell
# 1. Install Rust from https://rustup.rs (run the installer)
# 2. Install Node.js from https://nodejs.org
# 3. Install Visual Studio C++ build tools:
#    https://visualstudio.microsoft.com/visual-cpp-build-tools/
#    Select "Desktop development with C++"

# 4. Install Tauri CLI
cargo install tauri-cli --version "^1.6"

# 5. Build
cd C:\path\to\screen-tracker
cargo tauri build

# Built installer is at:
# src-tauri\target\release\bundle\msi\Screen Tracker_0.1.0_x64_en-US.msi
# Run the .msi to install. The app auto-starts on login after install.
```

### First-run setup (both machines)

1. Launch the app — a clock icon appears in the menu bar (Mac) or system tray (Windows).
2. Click the icon → click **Setup…** in the menu.
3. Fill in:
   - **Device name**: `Mac` or `Windows` (this appears in your reports)
   - **Service account JSON**: paste the full contents of the `.json` file from Step 1
   - **Drive folder ID**: from Step 2
   - **Gmail address**: your Gmail
   - **App password**: from Step 3
4. Click **Test connection** — it should say "Connected!" and create `config.json` in your Drive folder.
5. Click **Save & close**.

---

## Step 5 — Customize config.json in your Drive folder

After first-run setup, open your Drive folder. You'll see `config.json` — open it and edit as needed.
The file has inline comments explaining every field. Key things to change:

- `report_email` — set this to YOUR email address (where you want reports sent)
- `school_schedule` — adjust the 9–3 windows or remove them for specific days
- `overrides` — add holiday/PD day blocks (see examples in the file)
- `limit_hours` — change from 2.5 to whatever you want

**Changes are picked up automatically within 60 minutes.**
To force an immediate reload: quit and relaunch the app.

---

## Step 6 — Google Sheets dashboard

1. Create a new Google Sheet in your Drive.
2. Extensions → Apps Script → delete existing code → paste contents of `setup/dashboard.gs`.
3. Change `FOLDER_ID` at the top to your Drive folder ID.
4. Save → Run → `onOpen` (grant permissions when asked).
5. Reload the Sheet — a **Tracker** menu appears.
6. Click **Tracker → Refresh Dashboard** to pull all data.

Optional: set a daily auto-refresh trigger:
- In Apps Script: Triggers (clock icon) → Add Trigger → `refreshDashboard` → Time-driven → Day timer → choose a time.

---

## Troubleshooting

| Problem | Fix |
|---|---|
| App not detecting active window on Mac | System Settings → Privacy & Security → Accessibility → add Screen Tracker |
| "Auth failed" on Drive test | Double-check the service account JSON was pasted completely, including the opening `{` |
| "Drive write failed" | Make sure the Drive folder is shared with the service account email as **Editor** |
| No email received | Check Gmail App Password is correct; check spam folder; ensure 2FA is on |
| Config changes not taking effect | Wait up to 60 min, or quit and relaunch the app |
| App not starting at login on Mac | System Settings → General → Login Items → add Screen Tracker |

---

## File locations

| File | Location |
|---|---|
| Local database | `~/Library/Application Support/com.screentracker.ScreenTracker/tracker.db` (Mac) |
| Local credentials | `~/Library/Application Support/com.screentracker.ScreenTracker/credentials.json` (Mac) |
| Local database (Win) | `C:\Users\<user>\AppData\Roaming\com.screentracker\ScreenTracker\data\tracker.db` |
| Drive config | `ScreenTracker Data/config.json` (in your personal Drive) |
| Drive data | `ScreenTracker Data/Mac_YYYY-MM-DD.json` etc. |
