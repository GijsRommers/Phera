--TEST--
Magic file
--FILE--
<?php
echo basename(eval('return __FILE__;')), "\n";
--EXPECT--
magic-file.php(2) : eval()'d code
