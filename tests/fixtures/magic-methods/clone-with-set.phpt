--TEST--
Clone-with writes a hidden property through __set
--FILE--
<?php
class Copy {
    private $hidden;
    function __set($n, $v) { echo "$n=$v"; }
}
clone(new Copy, ["hidden" => 4]);
--EXPECT--
hidden=4
