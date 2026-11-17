--TEST--
include_once
--FILE--
<?php
$x=1;echo include_once __DIR__.'/shared.inc';echo include_once __DIR__.'/shared.inc';echo $x;
--EXPECT--
inc4213