--TEST--
Construction returns the receiver and direct constructor calls return a generator
--FILE--
<?php
class Source {
    public function __construct() {
        echo "generator body\n";
        yield 4;
    }
}
$source = new Source;
echo get_class($source), "\n";
$generator = $source->__construct();
echo get_class($generator), "\n";
var_dump($generator->current());
--EXPECT--
Source
Generator
generator body
int(4)
