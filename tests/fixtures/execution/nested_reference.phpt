--TEST--
nested_reference
--FILE--
<?php
$a=['x'=>['y'=>1]];$b=&$a['x']['y'];$b=42;echo $a['x']['y'];
--EXPECT--
42