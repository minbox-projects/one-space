# Archived notes instructions

Read `../README.md` for the authoritative archive policy and sealing procedure. This directory holds frozen historical implemented records, not current guidance by default.

Archived records keep their historical title punctuation, blank lines, language switcher, sections and old structure on both sides; active body format and structure-mirror rules never apply, and archived outbound links are never checked or repaired. Archive validation requires a complete triplet at a legal path, `Status: implemented` with the archive date immediately after it, identical archive dates on both sides that are not earlier than the file-name date, a valid consistency record whose recorded hashes match both sides and complete manifest entries.

Do not modify, move, delete or reorder a sealed English/Chinese/consistency-record triplet, or repair its outbound links. New archival work must preserve existing manifest entries and follow the README's review and append-only sealing procedure; sealing registers all three artifacts of each triplet. Use the true pre-change commit for any `--base` seal. Never rebuild the manifest to accept changed history; report integrity failures.
