--TEST--
Constructor precedes arguments
--FILE--
<?php
spl_autoload_register(function($name) { echo "load\n"; class Wanted { function __construct($a) { echo "construct\n"; } } }); function arg() { echo "argument\n"; return 1; } new Wanted(arg());
--EXPECT--
load
argument
construct
