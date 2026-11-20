--TEST--
gc-disabled-around-work
--FILE--
<?php
// Symfony's VarCloner turns the collector off around its work and back on
// after it: done 100,000 times, nothing is collected while it is off and
// nothing is left behind once it is back on.
final class Node {
    public static int $alive = 0;
    public static int $destructed = 0;
    public ?Node $other = null;
    public function __construct() { self::$alive++; }
    public function __destruct() { self::$alive--; self::$destructed++; }
}
function cloneLike(int $round): void {
    $gc = gc_enabled();
    if ($gc) {
        gc_disable();
    }
    try {
        if ($round % 4 === 0) {
            // A cycle of two nodes that only the cycle collector can free.
            $a = new Node;
            $b = new Node;
            $a->other = $b;
            $b->other = $a;
        }
    } finally {
        if ($gc) {
            gc_enable();
        }
    }
}
gc_disable();
$runs = gc_status()['runs'];
for ($i = 0; $i < 100_000; $i++) {
    cloneLike($i);
}
echo "while disabled: enabled=", var_export(gc_enabled(), true),
    " alive=", Node::$alive, " destructed=", Node::$destructed,
    " runs unchanged=", var_export(gc_status()['runs'] === $runs, true), "\n";
gc_enable();
gc_collect_cycles();
echo "after collecting: alive=", Node::$alive, " destructed=", Node::$destructed, "\n";

for ($i = 0; $i < 10_000; $i++) {
    cloneLike($i);
}
gc_collect_cycles();
echo "toggling while enabled: enabled=", var_export(gc_enabled(), true),
    " alive=", Node::$alive, " destructed=", Node::$destructed, "\n";
--EXPECT--
while disabled: enabled=false alive=50000 destructed=0 runs unchanged=true
after collecting: alive=0 destructed=50000
toggling while enabled: enabled=true alive=0 destructed=55000
