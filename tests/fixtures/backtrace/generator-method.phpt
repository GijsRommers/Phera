--TEST--
A generator method frame holds the same Generator receiver
--FILE--
<?php
function inspect($value) {
    yield debug_backtrace();
}
$generator = inspect('sample');
$trace = $generator->current();
echo implode(',', array_keys($trace[0])), "\n";
echo implode(',', array_keys($trace[1])), "\n";
var_dump($trace[0]['args'][0], $trace[1]['object'] === $generator);
--EXPECT--
function,args
file,line,function,class,object,type,args
string(6) "sample"
bool(true)
