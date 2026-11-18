--TEST--
jsonserializable null
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
echo json_encode(new Box(null)), "\n";
--EXPECT--
serialize
null
