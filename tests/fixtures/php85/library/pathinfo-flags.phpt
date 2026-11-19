--TEST--
pathinfo flags must select one field or PATHINFO_ALL
--FILE--
<?php
foreach ([PATHINFO_BASENAME, 0, PATHINFO_DIRNAME | PATHINFO_BASENAME, 16] as $flags) {
    try { var_dump(pathinfo('root/demo.txt', $flags)); }
    catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
}
--EXPECT--
string(8) "demo.txt"
string(0) ""
string(4) "root"
string(0) ""
