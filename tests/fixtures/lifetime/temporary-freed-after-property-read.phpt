--TEST--
temporary-freed-after-property-read
--FILE--
<?php
// A temporary object is freed right after a property or element is read
// from it, before the value is used.
class T { public $list; function __construct(public $n) { $this->list = [$n]; } function __destruct() { echo "-{$this->n}\n"; } }
function make($n) { return new T($n); }
echo (new T('chain'))->n, "\n";
echo make('call')->n, "\n";
echo make('index')->list[0], "\n";
$len = strlen(make('argument')->n);
echo "length $len\n";
if (make('condition')->n === 'condition') { echo "in the body\n"; }
echo "end\n";
--EXPECT--
-chain
chain
-call
call
-index
index
-argument
length 8
-condition
in the body
end
