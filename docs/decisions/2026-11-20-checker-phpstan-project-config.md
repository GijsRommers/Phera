# Read the supported PHPStan project settings before Phera settings

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

## Decision

Look for the three PHPStan NEON filenames in PHPStan's discovery order.
Read the supported list, string, and key settings and resolve local includes
relative to the file that declares them. Apply Phera settings next and
command-line options last. Report each unknown include once.

## Why

An existing PHPStan project should be able to run the checker with the
same paths, level, exclusions, baseline, and known rule groups.

## Turned down

Running PHP configuration, booting a PHPStan extension, or implementing
a general NEON evaluator.
