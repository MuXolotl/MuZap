# Third-Party Notices (MuZap)

This project contains original code and configuration, but it also ships (or uses) third‑party components.
Those components are governed by their own licenses.

This file is NOT a license. It is a transparency/attribution document.

## 1) Upstream projects

### 1.1 zapret (bol-van)
- Origin: bol-van/zapret
- License: MIT
- Notes: MuZap is based on the Windows "winws" (nfqws for Windows) ecosystem and configuration approach coming from zapret-based projects.

### 1.2 zapret-discord-youtube (Flowseal)
- Origin: Flowseal/zapret-discord-youtube
- License: MIT
- Notes: MuZap is a fork/derivative work in terms of scripts/configuration approach.

## 2) Bundled binaries (IMPORTANT)

Your antivirus may flag such tools as "riskware" or "hacktool" because they can intercept/modify traffic.
That does not automatically mean the files are malicious. Always verify what you run.

### 2.1 WinDivert (driver + DLL)
- Files in this repository:
  - `bin/WinDivert.dll`
  - `bin/WinDivert64.sys`
- Origin: basil00/WinDivert (official binaries are also published on reqrypt.org)
- License: dual-licensed (you may choose one):
  1) GNU LGPL v3 (or later), OR
  2) GNU GPL v2 (or later)
- Notes:
  - MuZap does not claim WinDivert is "MIT". WinDivert remains under its own license terms.
  - If you redistribute WinDivert binaries, you must comply with the chosen license (LGPL is typically used for redistribution scenarios).

### 2.2 winws.exe (zapret winws)
- File in this repository:
  - `bin/winws.exe`
- Origin: bol-van "zapret-win-bundle" (winws bundle for Windows), and ultimately bol-van/zapret
- License: MIT (as part of zapret ecosystem, see upstream license)
- Notes:
  - Always verify binary origin and integrity (checksums).
  - MuZap releases should include SHA256 checksums to allow users to verify downloaded archives.

### 2.3 cygwin1.dll (Cygwin runtime)
- File in this repository:
  - `bin/cygwin1.dll`
- Origin: Cygwin project
- License: LGPL v3 (or later) for the Cygwin API library + a "Linking Exception" (see Cygwin licensing terms).
- Notes:
  - If this DLL is not actually required for MuZap/winws operation, consider removing it to reduce licensing/compliance surface.
  - If it IS required and redistributed, ensure you comply with Cygwin redistribution requirements.

## 3) No legal advice

This document is provided for convenience and transparency and is not legal advice.
If you need legal certainty for redistribution in your jurisdiction, consult a qualified lawyer.
