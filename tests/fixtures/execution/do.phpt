--TEST--
do
--FILE--
<?php
$i=0; do {echo ++$i;} while($i<5);
--EXPECT--
12345