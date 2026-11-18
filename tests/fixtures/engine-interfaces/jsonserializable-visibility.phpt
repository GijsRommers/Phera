--TEST--
jsonserializable visibility
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
class C implements JsonSerializable {private $x=1; public $y=2; function jsonSerialize():mixed {return $this;}} echo json_encode(new C),"\n";
--EXPECT--
{"y":2}
