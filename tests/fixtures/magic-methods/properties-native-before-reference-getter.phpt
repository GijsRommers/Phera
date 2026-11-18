--TEST--
Taking a DateInterval property reference does not invoke a subclass getter
--FILE--
<?php
class Duration extends DateInterval {
    function &__get($name) { echo 'get'; return $GLOBALS['value']; }
}
$value = 8;
$d = new Duration('P2D');
$reference =& $d->d;
var_dump($reference);
--EXPECT--
int(2)
