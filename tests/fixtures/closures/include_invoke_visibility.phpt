--TEST--
Included invoke visibility warnings retain their declaration location
--FILE--
<?php
set_error_handler(function ($level, $message, $file, $line) {
    echo $message, ':', basename($file), ':', $line, "\n";
    return true;
});
include __DIR__ . '/private-invoke.inc';
var_dump((new IncludedInvoke)());
--EXPECT--
The magic method IncludedInvoke::__invoke() must have public visibility:private-invoke.inc:2
int(3)
