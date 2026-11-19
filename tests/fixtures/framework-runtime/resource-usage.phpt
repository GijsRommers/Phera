--TEST--
Kernel resource usage includes process and child counters
--FILE--
<?php
foreach ([0, 1] as $mode) {
    $fields = getrusage($mode);
    var_dump(is_int($fields['ru_maxrss']), is_int($fields['ru_utime.tv_sec']), count($fields));
}
$load = sys_getloadavg();
var_dump(count($load), is_float($load[0]), is_float($load[1]), is_float($load[2]));
--EXPECT--
bool(true)
bool(true)
int(17)
bool(true)
bool(true)
int(17)
int(3)
bool(true)
bool(true)
bool(true)
