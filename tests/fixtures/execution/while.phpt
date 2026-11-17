--TEST--
while
--FILE--
<?php
$i=0; while($i<5) echo $i++;
--EXPECT--
01234