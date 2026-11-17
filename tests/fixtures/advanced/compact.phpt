--TEST--
compact
--FILE--
<?php
$a=1;$b=2;foreach(compact('a','b') as $k=>$v)echo $k,$v;
--EXPECT--
a1b2