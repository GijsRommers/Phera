# Apply recognized PHPStan includes in their declared order

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

Apply a recognized Composer package at its position in the NEON include
list, alongside the other files. Later includes and the parent file can
override the flags contributed by earlier includes.
