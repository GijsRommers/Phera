--TEST--
An array callback preserves references delivered to __call
--FILE--
<?php
function increment(&$n) { ++$n; }
class Proxy {
    function __call($name, $args) { call_user_func_array('increment', $args); }
}
$i = 1;
call_user_func_array([new Proxy, 'run'], [&$i]);
echo $i;
--EXPECT--
2
