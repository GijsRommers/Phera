--TEST--
Nanosecond waits and absolute sleep validate their arguments
--FILE--
<?php
var_dump(time_nanosleep(0, 0), time_sleep_until(microtime(true) + 0.1));
foreach ([[-1, 0], [0, 1000000000], [0, -1]] as [$s, $n]) {
    try { time_nanosleep($s, $n); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
}
try { time_sleep_until(NAN); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
bool(true)
bool(true)
time_nanosleep(): Argument #1 ($seconds) must be greater than or equal to 0
Nanoseconds was not in the range 0 to 999 999 999 or seconds was negative
time_nanosleep(): Argument #2 ($nanoseconds) must be greater than or equal to 0
time_sleep_until(): Argument #1 ($timestamp) must be between 0 and 18446744073
