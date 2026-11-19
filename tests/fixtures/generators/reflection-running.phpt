--TEST--
A running generator and its running delegate can be inspected reentrantly
--FILE--
<?php
function inner(ReflectionGenerator $outer) {
    describe($outer);
    yield 1;
}
function describe(ReflectionGenerator $reflection): void {
    $leaf = new ReflectionGenerator($reflection->getExecutingGenerator());
    echo $reflection->getExecutingLine(), '/', $leaf->getFunction()->getName(), '/',
         implode(',', array_column($reflection->getTrace(DEBUG_BACKTRACE_IGNORE_ARGS), 'function')), "
";
    var_dump($reflection->getThis(), $reflection->getExecutingFile() === __FILE__);
}
function outer() {
    global $reflection;
    describe($reflection);
    yield from inner($reflection);
}
$generator = outer();
$reflection = new ReflectionGenerator($generator);
$generator->current();
--EXPECT--
15/outer/outer
NULL
bool(true)
16/inner/inner,outer
NULL
bool(true)
