--TEST--
spaceship_0
--FILE--
<?php
echo (int)(7 <=> 3), "\n";
--EXPECT--
1
