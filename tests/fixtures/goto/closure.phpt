--TEST--
A closure has its own label table
--FILE--
<?php
$run = function ($name) {
    goto entry;
    echo "miss\n";
    entry: echo $name, "\n";
};
goto entry;
entry:
$run("closure");
--EXPECT--
closure
