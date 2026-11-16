--TEST--
include
--FILE--
<?php
$x=1;$r=include __DIR__.'/shared.inc';echo $x,$r,$created;
--EXPECT--
inc3429