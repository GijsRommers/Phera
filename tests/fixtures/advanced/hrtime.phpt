--TEST--
hrtime reads the system's monotonic clock, not the time since the first call
--FILE--
<?php
$first = hrtime(true);
$parts = hrtime();
$second = hrtime(true);

// A large reading on the very first call: the clock started before this script.
var_dump(is_int($first));
var_dump($first > 1000000);
var_dump($second >= $first);
var_dump(hrtime(true) <= hrtime(true));

// The array form describes the same clock.
var_dump(count($parts));
var_dump(is_int($parts[0]));
var_dump(is_int($parts[1]));
var_dump($parts[1] >= 0 && $parts[1] < 1000000000);
var_dump(abs($parts[0] - intdiv($first, 1000000000)) <= 1);

// `false` keeps the array form.
var_dump(is_array(hrtime(false)));
var_dump(count(hrtime(false)));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
int(2)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(2)
