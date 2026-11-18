--TEST--
An array comparison tracks cycles through object properties
--FILE--
<?php
$o = new stdClass;
$p = new stdClass;
$a = [$o];
$b = [$p];
$o->items = $a;
$p->items = $b;
try { var_dump($a == $b); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
