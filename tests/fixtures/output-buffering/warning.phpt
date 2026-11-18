--TEST--
Displayed warnings are part of the active output buffer
--FILE--
<?php
ob_start();
echo $missing;
$value = ob_get_clean();
echo 'caught: ', preg_replace('/ in .+ on line /', ' on line ', $value);
--EXPECT--
caught: 
Warning: Undefined variable $missing on line 3
