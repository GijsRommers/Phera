--TEST--
A property that is the last holder of a reference prints by value
--FILE--
<?php
class Report {
    public string $value;
    function __get($name) { throw new LogicException($name); }
    static function make($properties) {
        $o = new self;
        foreach ($properties as $key => &$value) { $o->{$key} =& $value; }
        return $o;
    }
}
var_dump(Report::make(['value' => 'value']));
--EXPECT--
object(Report)#1 (1) {
  ["value"]=>
  string(5) "value"
}
