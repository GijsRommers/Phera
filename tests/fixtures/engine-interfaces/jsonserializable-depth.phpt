--TEST--
jsonserializable depth
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
var_dump(json_encode(new Box([[]]),0,1),json_last_error());
--EXPECT--
serialize
bool(false)
int(1)
