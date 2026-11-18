--TEST--
jsonserializable plain property nesting
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
echo json_encode((object)["x"=>new Box(7)]), "\n";
--EXPECT--
serialize
{"x":7}
