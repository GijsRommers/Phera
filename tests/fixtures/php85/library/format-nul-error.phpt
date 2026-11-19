--TEST--
A NUL byte used as a format specifier is an error
--FILE--
<?php
foreach ([fn() => sprintf("%\0", 1), fn() => vsprintf("%\0", [1])] as $format) {
    try { $format(); } catch (ValueError $error) { echo bin2hex($error->getMessage()), PHP_EOL; }
}
--EXPECT--
556e6b6e6f776e20666f726d6174207370656369666965722022
556e6b6e6f776e20666f726d6174207370656369666965722022
