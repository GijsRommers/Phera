# Speed is compared with PHP started as `php -n` (no Xdebug), with a second column for opcache and the JIT, and reported in cycles or CPU time next to instructions

- Date: 2026-11-19
- Area: Speed and method
- Decided by: Design

## Decision

Speed is compared with PHP started as `php -n` (no Xdebug), with a second column for opcache and the JIT, and reported in cycles or CPU time next to instructions.

## Why

An earlier comparison used a PHP with Xdebug loaded and made Phera look faster than it is. Phera also retires fewer instructions per cycle than PHP, so instruction counts alone understate the gap.

## Turned down

Instruction counts as the only measure.
