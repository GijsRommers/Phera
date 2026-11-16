--TEST--
array_spread
--FILE--
<?php
$a=[1,...[2,3],'x'=>4];foreach($a as $k=>$v)echo $k,$v;
--EXPECT--
011223x4