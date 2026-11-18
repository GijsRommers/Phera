--TEST--
Nested array comparisons preserve enum identity and native comparison hooks
--FILE--
<?php
enum Flag { case On; case Off; }
$one = new DateTimeImmutable('2026-01-01T12:00:00+00:00');
$two = new DateTimeImmutable('2026-01-01T13:00:00+01:00');
var_dump(
    [[$one, Flag::On]] == [[$two, Flag::On]],
    [[$one, Flag::On]] == [[$two, Flag::Off]],
    [[$one, Flag::On]] == [[$two, $one]]
);
--EXPECT--
bool(true)
bool(false)
bool(false)
