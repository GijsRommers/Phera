--TEST--
jsonserializable recursion
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
$box=new Box(null); $box->input=[$box]; var_dump(json_encode($box),json_last_error());
--EXPECT--
serialize
bool(false)
int(6)
