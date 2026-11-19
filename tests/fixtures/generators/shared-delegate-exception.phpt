--TEST--
The resuming generator receives the original exception and other consumers see the aborted delegate
--FILE--
<?php
function source() {
    yield 12;
    throw new LogicException('source failed');
}
function proxy($source) {
    try { return yield from $source; }
    catch (Exception $error) { yield get_class($error) . ': ' . $error->getMessage(); }
}
$source = source();
$first = proxy($source);
$second = proxy($source);
var_dump($first->current(), $second->current());
$first->next();
var_dump($first->current(), $second->current());
--EXPECT--
int(12)
int(12)
string(29) "LogicException: source failed"
string(83) "ClosedGeneratorException: Generator yielded from aborted, no return value available"
