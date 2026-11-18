--TEST--
A label may precede a function or class declaration
--FILE--
<?php
goto declarations;
declarations:
function work() { echo "function\n"; }
class Worker {}
work();
echo Worker::class, "\n";
--EXPECT--
function
Worker
