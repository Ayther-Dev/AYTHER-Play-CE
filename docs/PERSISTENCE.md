# Persistence and pack isolation

`saves::write_atomic` creates a unique `.save-*` file in the destination directory,
writes all bytes, calls `sync_all`, and publishes it using `NamedTempFile::persist`.
An existing destination is never deleted first. Write, sync and replacement errors
propagate through `write_version`, restoration and portable-save import. The UI
restoration path displays errors. Legacy migration counts only successful writes
and keeps the source after a write failure.

On failure, RAII attempts to remove the temporary file. Cleanup failure or process
termination can leave a `.save-*` file, which is not recognized as a saved version.
With Play closed, it can be inspected for recovery or deleted; Play never promotes
it automatically. Cleanup errors do not replace the original persistence error.
An interrupted temporary write or failed replacement leaves the prior destination
accessible. Tests inject both failures and inspect the old bytes and directory.

Atomic publication is not power-loss durability. Play does not synchronize the
containing directory and has not validated storage-controller/filesystem behavior
under sudden power loss. Nor does this helper establish guarantees for save files
written independently by Runtime. Concurrent writers publish whole files, but the
last successful replacement wins.

All mutating pack entry points validate the identifier and all five destinations:
`.ay`, `.ay.prev`, `.ay.new`, `.ay.swap` and `.ay.part`. Identifiers contain 1–128
ASCII letters/digits, hyphens or underscores, with internal dots/spaces allowed
for ordinary pack filenames. Leading/trailing dots/spaces and Windows device
names (including names with extensions) are excluded.
Absolute paths, navigation, separators, stream syntax and empty identifiers fail
before filesystem mutation, including when entered through the CLI.

Destinations must be direct children of the authorized pack directory. Existing
nonregular files, symbolic links and Windows reparse points are rejected; existing
paths are canonicalized and checked against the canonical root. The configured
root defines the authorized directory, including if the user configured a link.
This is not a sandbox against another process replacing directory entries between
validation and mutation: the pack directory must not be concurrently modified.
