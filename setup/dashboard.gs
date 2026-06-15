/**
 * Screen Tracker — Google Sheets Dashboard
 *
 * HOW TO SET UP:
 * 1. Create a new Google Sheet in your personal Drive (same account that owns the tracker folder).
 * 2. Click Extensions → Apps Script.
 * 3. Delete the default code and paste this entire file.
 * 4. Set FOLDER_ID below to your tracker Drive folder ID.
 * 5. Click Run → onOpen once to grant permissions.
 * 6. A "Tracker" menu will appear in your Sheet — click "Refresh Dashboard" any time.
 * 7. Optional: set a time-based trigger (Triggers → Add Trigger → refreshDashboard, daily).
 */

const FOLDER_ID = "YOUR_DRIVE_FOLDER_ID_HERE"; // ← paste your folder ID

// ── Entry points ──────────────────────────────────────────────────────────────

function onOpen() {
  SpreadsheetApp.getUi()
    .createMenu("Tracker")
    .addItem("Refresh Dashboard", "refreshDashboard")
    .addItem("Show Last 7 Days", "showLast7Days")
    .addToUi();
}

function refreshDashboard() {
  const files = getTrackerFiles();
  const summaries = files.map(parseFile).filter(Boolean);
  writeSummarySheet(summaries);
  writeAppBreakdownSheet(summaries);
  SpreadsheetApp.getActiveSpreadsheet().toast("Dashboard refreshed — " + summaries.length + " data file(s) loaded.", "Screen Tracker", 4);
}

function showLast7Days() {
  const files = getTrackerFiles(7);
  const summaries = files.map(parseFile).filter(Boolean);
  writeSummarySheet(summaries, "Last 7 Days");
  writeAppBreakdownSheet(summaries, "Apps (7 Days)");
}

// ── Data loading ──────────────────────────────────────────────────────────────

function getTrackerFiles(daysBack) {
  const folder = DriveApp.getFolderById(FOLDER_ID);
  const files = [];
  const iter = folder.getFiles();
  const cutoff = daysBack ? new Date(Date.now() - daysBack * 86400000) : null;

  while (iter.hasNext()) {
    const file = iter.next();
    const name = file.getName();
    // Only pick up daily data files (device_YYYY-MM-DD.json), skip config.json
    if (name === "config.json") continue;
    if (cutoff && file.getLastUpdated() < cutoff) continue;
    files.push(file);
  }

  // Sort by filename (which encodes date)
  files.sort((a, b) => a.getName().localeCompare(b.getName()));
  return files;
}

function parseFile(file) {
  try {
    const data = JSON.parse(file.getBlob().getDataAsString());
    return {
      date: data.date || "",
      device: data.device_name || file.getName().split("_")[0],
      total_minutes: data.total_minutes || 0,
      exempt_minutes: data.exempt_minutes || 0,
      limited_minutes: data.limited_minutes || data.total_minutes || 0,
      app_breakdown: data.app_breakdown || {},
    };
  } catch (e) {
    return null;
  }
}

// ── Sheet writers ─────────────────────────────────────────────────────────────

function writeSummarySheet(summaries, sheetName) {
  sheetName = sheetName || "Dashboard";
  const ss = SpreadsheetApp.getActiveSpreadsheet();
  let sheet = ss.getSheetByName(sheetName);
  if (!sheet) sheet = ss.insertSheet(sheetName);
  sheet.clearContents();
  sheet.clearFormats();

  // Title
  sheet.getRange(1, 1).setValue("Screen Tracker — Daily Summary")
    .setFontSize(14).setFontWeight("bold");
  sheet.getRange(1, 2).setValue("Last refreshed: " + new Date().toLocaleString())
    .setFontColor("#888888").setFontSize(10);

  // Header row
  const headers = ["Date", "Device", "Tracked (min)", "Tracked (h)", "Exempt (min)", "Over limit?"];
  const headerRange = sheet.getRange(3, 1, 1, headers.length);
  headerRange.setValues([headers])
    .setFontWeight("bold")
    .setBackground("#1a1a2e")
    .setFontColor("#ffffff");

  if (summaries.length === 0) {
    sheet.getRange(4, 1).setValue("No data files found in folder.");
    return;
  }

  // Data rows — group by date for combined totals
  const byDate = {};
  for (const s of summaries) {
    if (!byDate[s.date]) byDate[s.date] = [];
    byDate[s.date].push(s);
  }

  let row = 4;
  for (const date of Object.keys(byDate).sort()) {
    const entries = byDate[date];
    for (const s of entries) {
      const limitMins = getLimitFromConfig() * 60;
      const over = s.limited_minutes >= limitMins;
      const rowData = [
        date,
        s.device,
        s.limited_minutes,
        (s.limited_minutes / 60).toFixed(2),
        s.exempt_minutes,
        over ? "YES" : "no",
      ];
      const r = sheet.getRange(row, 1, 1, rowData.length);
      r.setValues([rowData]);
      if (over) r.getCell(1, 6).setFontColor("#c0392b").setFontWeight("bold");
      row++;
    }

    // Combined row if 2 machines on same date
    if (entries.length > 1) {
      const total = entries.reduce((a, b) => a + b.limited_minutes, 0);
      const exempt = entries.reduce((a, b) => a + b.exempt_minutes, 0);
      const combinedRow = [date, "— TOTAL —", total, (total / 60).toFixed(2), exempt, ""];
      const r = sheet.getRange(row, 1, 1, combinedRow.length);
      r.setValues([combinedRow]).setBackground("#f0f0f5").setFontWeight("bold");
      row++;
    }
  }

  sheet.autoResizeColumns(1, headers.length);
  ss.setActiveSheet(sheet);
}

function writeAppBreakdownSheet(summaries, sheetName) {
  sheetName = sheetName || "App Breakdown";
  const ss = SpreadsheetApp.getActiveSpreadsheet();
  let sheet = ss.getSheetByName(sheetName);
  if (!sheet) sheet = ss.insertSheet(sheetName);
  sheet.clearContents();
  sheet.clearFormats();

  sheet.getRange(1, 1).setValue("App Usage Breakdown (foreground time, exempt hours excluded)")
    .setFontSize(13).setFontWeight("bold");

  const headers = ["Date", "Device", "App", "Minutes", "Hours"];
  sheet.getRange(3, 1, 1, headers.length)
    .setValues([headers])
    .setFontWeight("bold")
    .setBackground("#1a1a2e")
    .setFontColor("#ffffff");

  let row = 4;
  for (const s of summaries) {
    const apps = Object.entries(s.app_breakdown).sort((a, b) => b[1] - a[1]);
    for (const [app, mins] of apps) {
      sheet.getRange(row, 1, 1, 5).setValues([[
        s.date, s.device, app, mins, (mins / 60).toFixed(2)
      ]]);
      row++;
    }
  }

  sheet.autoResizeColumns(1, headers.length);
}

function getLimitFromConfig() {
  // Try to read limit_hours from config.json in the folder; fall back to 2.5
  try {
    const folder = DriveApp.getFolderById(FOLDER_ID);
    const iter = folder.getFilesByName("config.json");
    if (iter.hasNext()) {
      const cfg = JSON.parse(iter.next().getContentText());
      return cfg.limit_hours || 2.5;
    }
  } catch (e) {}
  return 2.5;
}
