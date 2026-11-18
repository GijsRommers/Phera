--TEST--
jsonserializable recursion throws
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
$box=new Box(null); $box->input=[$box]; try{json_encode($box,JSON_THROW_ON_ERROR);}catch(JsonException $e){var_dump($e->getCode(),$e->getMessage());}
--EXPECT--
serialize
int(6)
string(18) "Recursion detected"
