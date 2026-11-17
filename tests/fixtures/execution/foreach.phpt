--TEST--
foreach
--FILE--
<?php
foreach([3,4,5] as $k=>$v)echo $k,$v;
--EXPECT--
031425