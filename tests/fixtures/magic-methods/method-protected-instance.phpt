--TEST--
method protected instance
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

$o->protectedMethod(1);
--EXPECT--
protectedMethod:1
