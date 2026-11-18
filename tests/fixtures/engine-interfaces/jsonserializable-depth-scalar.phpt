--TEST--
jsonserializable depth scalar
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
var_dump(json_encode(new Box(7),0,0),json_last_error());
--EXPECT--
serialize
string(1) "7"
int(0)
