--TEST--
Returning the same case from jsonSerialize encodes its object properties
--FILE--
<?php
enum Flag: int implements JsonSerializable {
    case On = 1;
    public function jsonSerialize(): mixed { return $this; }
}
echo json_encode(Flag::On), "\n";
--EXPECT--
{"name":"On","value":1}
