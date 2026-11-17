--TEST--
function_hoist
--FILE--
<?php
echo twice(3);function twice($x){return $x*2;}
--EXPECT--
6