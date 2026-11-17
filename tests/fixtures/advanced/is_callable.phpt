--TEST--
is_callable
--FILE--
<?php
function f(){} echo (int)is_callable('f'),(int)is_callable('absent');
--EXPECT--
10