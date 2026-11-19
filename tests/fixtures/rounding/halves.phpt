--TEST--
Directed and halfway enum rounding handle both signs
--FILE--
<?php
foreach (RoundingMode::cases() as $case) {
    echo $case->name, ': ';
    var_dump(round(2.5, mode: $case), round(-2.5, 0, $case));
}
--EXPECT--
HalfAwayFromZero: float(3)
float(-3)
HalfTowardsZero: float(2)
float(-2)
HalfEven: float(2)
float(-2)
HalfOdd: float(3)
float(-3)
TowardsZero: float(2)
float(-2)
AwayFromZero: float(3)
float(-3)
NegativeInfinity: float(2)
float(-3)
PositiveInfinity: float(3)
float(-2)
