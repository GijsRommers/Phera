--TEST--
RoundingMode exposes eight canonical UnitEnum cases
--FILE--
<?php
var_dump(enum_exists('RoundingMode'));
foreach (RoundingMode::cases() as $case) {
    echo $case->name, ': ', (int) ($case === constant('RoundingMode::'.$case->name)), "\n";
}
--EXPECT--
bool(true)
HalfAwayFromZero: 1
HalfTowardsZero: 1
HalfEven: 1
HalfOdd: 1
TowardsZero: 1
AwayFromZero: 1
NegativeInfinity: 1
PositiveInfinity: 1
