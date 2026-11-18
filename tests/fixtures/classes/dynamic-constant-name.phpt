--TEST--
Dynamic class constant fetch uses the computed member name
--FILE--
<?php
class Period { public const Days = 7; }
$class = 'Period';
$part = 'D';
var_dump(Period::{$part . 'ays'}, $class::{$part . 'ays'});
--EXPECT--
int(7)
int(7)
