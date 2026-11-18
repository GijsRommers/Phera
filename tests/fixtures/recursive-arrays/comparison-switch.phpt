--TEST--
switch reports recursion while comparing a case
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
try { switch ($a) { case $b: echo "matched\n"; break; default: echo "default\n"; } } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
