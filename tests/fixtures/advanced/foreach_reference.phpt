--TEST--
foreach_reference
--FILE--
<?php
$a=[1,2,3];foreach($a as &$v)$v*=2;unset($v);foreach($a as $v)echo $v;
--EXPECT--
246