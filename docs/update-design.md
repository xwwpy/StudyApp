# Windows portable update design

The update entry points run on a background worker. The UI checks only the public
`xwwpy/StudyApp` latest stable release and shows the installed version, proposed
version and release notes. Checking does not download or replace the application.
An explicit update click authorizes preparation. Before launching the handoff,
the UI must persist every draft and resolve dirty settings, then quit normally.

## Release and transport contract

- API: `https://api.github.com/repos/xwwpy/StudyApp/releases/latest`, without a
  token, with a fixed User-Agent and GitHub JSON Accept header.
- Compare semantic versions. Drafts, prereleases, build-metadata package names,
  equal versions and older releases cannot produce an update transaction.
- Require exactly one expected `WW-StudyApp-<version>-windows-x86_64.zip` and its
  `.zip.sha256` companion. Initial asset URLs must exactly match this repository,
  the selected tag and the expected filenames.
- Require a SHA256 GitHub asset digest for both downloads. Hash the package and
  compare it with both its API digest and the single, filename-bound checksum
  line. Missing digests fail closed, including older releases without digests.
- Limit API/checksum/package sizes; package compressed size is at most 256 MiB.
  All connections use HTTPS with bounded connection and overall request timeouts.
  Follow at most three redirects to exact official GitHub release hosts; send no
  credentials or cookies. Signed redirect URLs never enter user-facing errors.

GitHub documents its stable-release API and SHA256 asset `digest` in the
[release API](https://docs.github.com/en/rest/releases/releases), and its binary
download/redirect behavior in the
[release assets API](https://docs.github.com/en/rest/releases/assets).

## Preparation and archive policy

Use `current_exe()` to identify the installation; preserve a renamed executable.
Create a unique app-owned temporary staging directory. Download, hash, validate
and extract there before probing installation writability. The trusted helper is
embedded in the executable; it is never fetched from a release or extracted from
the package. Pass the helper a JSON plan through a literal `-File` invocation,
without constructing PowerShell command text. It uses Windows PowerShell 5.1 and
.NET ZipArchive, so no downloaded helper executable or ZIP crate is required.

The helper rejects absolute/drive/ADS/backslash paths, traversal, empty segments,
Windows device aliases, trailing dots/spaces, case-insensitive duplicate paths,
file/directory collisions, ZIP links and reparse attributes. Every member must
have the expected version root; exactly one `WW-StudyApp.exe` and a `LICENSE`
must be present. Limit entry count to 8,192, single-file size to 512 MiB, total
expanded size to 768 MiB, and expansion ratio for files above 1 MiB to 200:1.
Extraction uses create-new files and enforces the declared size while streaming.
No script from the ZIP is executed.

Preparation records hashes and the parent process start time. Application handoff
rechecks the package hash, all archive members, every extracted file against the
original archive bytes, and the previous executable hash/size/modification time.
File and ancestor-directory reparse points are rejected before writes. UNC and
reparse-backed installs use the manual update path. Installation permission
failure never triggers elevation.

## Replacement and recovery

1. Start the trusted helper hidden, after explicit user approval and draft/settings
   persistence. The helper waits at most five minutes for the recorded parent to
   exit normally. It never terminates the application. PID reuse is distinguished
   with the captured process start time.
2. Recheck the old executable identity. Copy the validated distribution documents,
   licenses and source into a fresh `.ww-studyapp-distribution-<version>-<id>`
   sibling directory. These files remain available with the updated application;
   existing user files and earlier distribution directories are untouched.
3. Copy the new executable to a unique sibling. Use same-directory
   `System.IO.File.Replace` to atomically replace the exact existing executable
   and retain its old contents as `.ww-studyapp-backup-<id>.exe`.
4. Write a structured status record, then restart the preserved executable path
   with the original `--data-dir` selection. Windows argument quoting handles
   spaces, Chinese text, embedded quotes and trailing backslashes.
5. On a post-replacement failure, verify the candidate and backup identities,
   copy the backup into a fresh sibling, and atomically restore the old executable.
   Retain the failed binary, backup and staging files, record rollback status,
   and attempt a normal restart of the restored executable. If recovery itself
   fails, retain the evidence and provide manual recovery guidance.

Only the single application executable is replaced. Runtime assets are embedded.
No existing directory, arbitrary user file or user data is deleted. Stages,
backups, previous status records and the tiny unique writability probe are retained.
The application can read the install-specific status record after restart.

## Verification boundaries

All replacement fixtures use dummy known files in fresh `.scratch` directories.
No test updates the real app or a user installation. Fixture evidence and exact
commands are recorded in `.scratch/product-update-report.md` during development.

Hash verification protects download integrity against inconsistent or corrupt
assets; it is not a publisher signing system independent of the GitHub account.
The helper detects immediate process-launch failure. It cannot detect a later
application crash or guarantee recovery from power loss after replacement.
Retained backups support manual recovery in these cases. No updater activity
requires administrator rights, and no actual release replacement is exercised
as part of fixture tests.
