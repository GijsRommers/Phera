--TEST--
Printed arguments abbreviate strings and arrays
--FILE--
<?php
function inspect(...$values) { debug_print_backtrace(); }
inspect("ab\ncdefghijklmnopqr", [], new stdClass(), false, null, 4.0, key: 'named');
--EXPECTF--
#0 %s/printed-arguments.php(3): inspect('ab\ncdefghijklmn...', Array, Object(stdClass), false, NULL, 4.0, key: 'named')
