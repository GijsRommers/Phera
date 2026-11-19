--TEST--
No caller is exposed at top level
--FILE--
<?php
echo json_encode(debug_backtrace(), JSON_UNESCAPED_SLASHES);
debug_print_backtrace();
--EXPECT--
[]
