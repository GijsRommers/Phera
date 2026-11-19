--TEST--
Negative precision rounds tens and hundreds
--FILE--
<?php
var_dump(round(1550, -2, RoundingMode::HalfEven));
var_dump(round(-150, -2, RoundingMode::TowardsZero));
var_dump(round(1e23, -2, RoundingMode::NegativeInfinity));
--EXPECT--
float(1600)
float(-100)
float(1.0E+23)
