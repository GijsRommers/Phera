--TEST--
jsonserializable duplicate
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
$box=new Box(1); echo json_encode([$box,$box]), "\n";
--EXPECT--
serialize
serialize
[1,1]
