--TEST--
generator-closed-at-end-of-request
--FILE--
<?php
// A suspended generator still alive when the script ends is destroyed in
// the destructor phases: its finally block runs, in the phases' order.
class Holder { function __construct(public Generator $gen) {} function __destruct() { echo "destruct holder\n"; } }
function gen(string $name) {
    try {
        echo "$name: before yield\n";
        yield;
    } finally {
        echo "$name: finally\n";
    }
}
$first = new Holder(gen('held'));
$first->gen->rewind();
$cycle = new stdClass;
$cycle->self = $cycle;
$cycle->gen = gen('in a cycle');
$cycle->gen->rewind();
unset($cycle);
$global = gen('global');
$global->rewind();
echo "end of script\n";
--EXPECT--
held: before yield
in a cycle: before yield
global: before yield
end of script
global: finally
destruct holder
held: finally
in a cycle: finally
