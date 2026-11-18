--TEST--
JsonSerializable supplies JSON for a pure enum
--FILE--
<?php
enum Flag implements JsonSerializable {
    case On;
    public function jsonSerialize(): mixed { return $this->name; }
}
var_dump(json_encode(Flag::On));
--EXPECT--
string(4) ""On""
