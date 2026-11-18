--TEST--
DateInterval native properties run ahead of user magic hooks
--FILE--
<?php
class Duration extends DateInterval {
    function __get($n) { echo "get $n\n"; return 12; }
    function __set($n, $v) { echo "set $n=$v\n"; }
    function __isset($n) { echo "isset $n\n"; return true; }
    function __unset($n) { echo "unset $n\n"; }
}
$d = new Duration("P2D");
var_dump($d->d, isset($d->d));
$d->d = 3;
var_dump($d->d, $d->extra);
$d->extra = 42;
unset($d->extra);
--EXPECT--
int(2)
bool(true)
get extra
int(3)
int(12)
set extra=42
unset extra
