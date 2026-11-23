--TEST--
jsonserializable recursion partial
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
$box=new Box(null); $box->input=[$box]; var_dump(json_encode($box,JSON_PARTIAL_OUTPUT_ON_ERROR),json_last_error());
--EXPECT--
serialize
string(6) "[null]"
int(6)
