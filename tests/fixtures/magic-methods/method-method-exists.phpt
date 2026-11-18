--TEST--
method method exists
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

var_dump(method_exists($o, "run"), method_exists($o, "hidden"));
--EXPECT--
bool(false)
bool(true)
