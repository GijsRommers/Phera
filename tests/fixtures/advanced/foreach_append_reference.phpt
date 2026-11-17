--TEST--
foreach_append_reference
--FILE--
<?php
$a=[1,2];foreach($a as &$v){echo $v;if($v==2)$a[]=3;}unset($v);
--EXPECT--
123