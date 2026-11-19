--TEST--
A method has its own label table
--FILE--
<?php
class Router {
    function run() {
        goto entry;
        echo "miss\n";
        entry: echo "method\n";
    }
}
goto entry;
entry:
(new Router())->run();
--EXPECT--
method
