--TEST--
Two distinct objects whose properties lead to themselves throw
--FILE--
<?php
$o = new stdClass;
$o->self = $o;
$p = clone $o;
$p->self = $p;
try { var_dump($o == $p); } catch (Error $e) { echo get_class($e), ": ", $e->getMessage(), "\n"; }
echo "done\n";
--EXPECT--
Error: Nesting level too deep - recursive dependency?
done
