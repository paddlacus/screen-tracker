/**
 * Screen Tracker — Apps Script Proxy
 *
 * This script acts as a secure bridge between the tracker app and your Google Drive.
 * It runs under YOUR Google account so it can write to your Drive without quota issues.
 *
 * SETUP:
 * 1. Go to script.google.com → New project → paste this entire file
 * 2. Set FOLDER_ID and SECRET below
 * 3. Click Deploy → New deployment → Web app
 *    - Execute as: Me
 *    - Who has access: Anyone
 * 4. Copy the Web app URL — paste it into the tracker Setup screen
 */

const FOLDER_ID = "YOUR_DRIVE_FOLDER_ID_HERE"; // ← your Drive folder ID
const SECRET    = "CHANGE_THIS_TO_A_RANDOM_STRING"; // ← make up any secret password

// ── Write a file (POST) ───────────────────────────────────────────────────────

function doPost(e) {
  try {
    const params = JSON.parse(e.postData.contents);
    if (params.secret !== SECRET) {
      return json({ ok: false, error: "unauthorized" });
    }

    const folder = DriveApp.getFolderById(FOLDER_ID);
    const filename = params.filename;
    const content  = params.content;

    const iter = folder.getFilesByName(filename);
    if (iter.hasNext()) {
      iter.next().setContent(content);
    } else {
      folder.createFile(filename, content, MimeType.PLAIN_TEXT);
    }

    return json({ ok: true });
  } catch (err) {
    return json({ ok: false, error: String(err) });
  }
}

// ── Read a file or list files (GET) ──────────────────────────────────────────

function doGet(e) {
  try {
    const p = e.parameter;
    if (p.secret !== SECRET) {
      return json({ ok: false, error: "unauthorized" });
    }

    const folder = DriveApp.getFolderById(FOLDER_ID);

    // List all filenames in the folder
    if (p.action === "list") {
      const files = [];
      const iter = folder.getFiles();
      while (iter.hasNext()) files.push(iter.next().getName());
      return json({ ok: true, files });
    }

    // Download a specific file
    const iter = folder.getFilesByName(p.filename);
    if (!iter.hasNext()) {
      return json({ ok: false, error: "not_found" });
    }

    const content = iter.next().getBlob().getDataAsString();
    return json({ ok: true, content });
  } catch (err) {
    return json({ ok: false, error: String(err) });
  }
}

function json(obj) {
  return ContentService
    .createTextOutput(JSON.stringify(obj))
    .setMimeType(ContentService.MimeType.JSON);
}
