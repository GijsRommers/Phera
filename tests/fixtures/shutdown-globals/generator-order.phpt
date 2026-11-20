--TEST--
Sole-owned and shared global generators close in PHP order
--FILE--
<?php
function watch($name) {
    try { yield; } finally { echo "closed $name\n"; }
}
$first = watch('first'); $first->rewind();
$other = watch('shared'); $other->rewind(); $alias = $other;
$second = watch('second'); $second->rewind();
$items = [watch('nested')]; $items[0]->rewind();
register_shutdown_function(fn () => print "shutdown\n");
--EXPECT--
shutdown
closed second
closed first
closed shared
closed nested
