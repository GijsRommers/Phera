--TEST--
isset_empty
--FILE--
<?php
echo (int)isset($a),(int)empty($a);$a=0;echo (int)isset($a),(int)empty($a);
--EXPECT--
0111