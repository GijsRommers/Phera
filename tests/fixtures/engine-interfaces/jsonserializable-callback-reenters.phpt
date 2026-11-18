--TEST--
jsonserializable callback reenters
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
class C implements JsonSerializable {function jsonSerialize():mixed {return json_encode($this);}} var_dump(json_encode(new C),json_last_error());
--EXPECT--
string(5) "false"
int(0)
