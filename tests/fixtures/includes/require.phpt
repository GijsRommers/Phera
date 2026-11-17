--TEST--
require
--FILE--
<?php
$x=1;echo require __DIR__.'/shared.inc';echo $x;
--EXPECT--
inc423