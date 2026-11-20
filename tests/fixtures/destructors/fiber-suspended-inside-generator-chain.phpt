--TEST--
fiber-suspended-inside-generator-chain
--FILE--
<?php
// A fiber suspended inside a `yield from` chain at the end of the script:
// destroying the fiber unwinds the chain, inner generator first. The outer
// generator is not closed on its own while its chain runs in the fiber.
function inner() {
    try {
        echo "inner: before suspend\n";
        Fiber::suspend();
        yield;
    } finally {
        echo "inner: finally\n";
    }
}
function outer() {
    try {
        yield from inner();
        yield;
    } finally {
        echo "outer: finally\n";
    }
}
$generator = outer();
$fiber = new Fiber(function () use ($generator) {
    $generator->current();
    echo "not reached\n";
});
$fiber->start();
echo "end of script\n";
--EXPECT--
inner: before suspend
end of script
inner: finally
outer: finally
