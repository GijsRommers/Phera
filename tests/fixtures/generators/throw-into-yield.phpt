--TEST--
Throw injects a Throwable at the innermost suspended yield
--FILE--
<?php
function inner() {
    try {
        yield "ready";
    } catch (LogicException $error) {
        yield $error->getMessage();
    } finally {
        echo "finally\n";
    }
    return "inner return";
}
function outer() {
    return yield from inner();
}
$generator = outer();
var_dump($generator->throw(new LogicException("injected")));
$generator->next();
var_dump($generator->getReturn());
--EXPECT--
string(8) "injected"
finally
string(12) "inner return"
