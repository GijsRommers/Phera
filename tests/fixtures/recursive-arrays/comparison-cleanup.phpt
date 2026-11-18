--TEST--
A comparison error leaves no recursion state behind
--FILE--
<?php
$a = [1];
$a[5] = &$a;
$b = [1];
$b[5] = &$b;
try { var_dump($a == $b); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
try { var_dump($a == $b); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
var_dump($a === $a);
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
Error: Nesting level too deep - recursive dependency?
done
bool(true)
