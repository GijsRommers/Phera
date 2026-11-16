--TEST--
foreach_copy
--FILE--
<?php
$a=[1,2];foreach($a as $v){echo $v;$a[]=3;}
--EXPECT--
12