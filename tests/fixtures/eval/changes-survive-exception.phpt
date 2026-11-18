--TEST--
Changes survive exception
--FILE--
<?php
$x=2; try { eval('$x=7; throw new Exception;'); } catch (Exception $e) {} var_dump($x);
--EXPECT--
int(7)
