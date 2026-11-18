--TEST--
jsonserializable scalar
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
echo json_encode(new Box(5)), "\n";
--EXPECT--
serialize
5
