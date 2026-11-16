--TEST--
require_once
--FILE--
<?php
$x=1;echo require_once __DIR__.'/shared.inc';echo require_once __DIR__.'/shared.inc';echo $x;
--EXPECT--
inc4213