--TEST--
scandir sorting order validation
--FILE--
<?php
mkdir('fixture-scandir-order');
try { var_dump(scandir('fixture-scandir-order', 42)); }
catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
rmdir('fixture-scandir-order');
--EXPECT--
scandir(): Argument #2 ($sorting_order) must be one of the SCANDIR_SORT_ASCENDING, SCANDIR_SORT_DESCENDING, or SCANDIR_SORT_NONE constants
