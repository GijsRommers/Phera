--TEST--
Adjacent float literals keep the sign of zero in round calls
--FILE--
<?php
var_dump(round(0.0), round(-0.0), round(-0.0, -1, RoundingMode::HalfEven));
--EXPECT--
float(0)
float(-0)
float(-0)
