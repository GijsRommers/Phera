--TEST--
A dynamic fetch reads an interface constant
--FILE--
<?php
interface Size { public const Large = 3; }
$member = 'Large';
var_dump(Size::{$member});
--EXPECT--
int(3)
