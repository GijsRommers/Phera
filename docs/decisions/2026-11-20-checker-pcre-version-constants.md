# Use Phera's PCRE version constants in checker comparisons

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Use the integer values registered by Phera's PCRE extension to prove
comparisons such as `PCRE_VERSION_MAJOR < 10`. The comparison message
contains the version that Phera exposes.
