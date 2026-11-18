--TEST--
method native callback
--FILE--
<?php
class Caller {
    private function hidden() { echo "actual"; }
    protected function protectedMethod() { echo "actual"; }
    function __call($name, $args) { echo $name, ":", implode(",", $args); }
    static function __callStatic($name, $args) { echo "static ", $name, ":", implode(",", $args); }
    function inside() { $this->hidden(); }
}
$o = new Caller;

array_map([$o, "run"], [1, 2]);
--EXPECT--
run:1run:2
