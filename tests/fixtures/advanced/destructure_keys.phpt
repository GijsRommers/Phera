--TEST--
destructure_keys
--FILE--
<?php
['x'=>$x,'y'=>$y]=['y'=>2,'x'=>1];echo $x,$y;
--EXPECT--
12