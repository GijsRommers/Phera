--TEST--
jsonserializable exception
--FILE--
<?php
class Box implements JsonSerializable {
    public $input;
    public function __construct($input) { $this->input=$input; }
    public function jsonSerialize(): mixed { echo "serialize\n"; return $this->input; }
}
class C implements JsonSerializable {function jsonSerialize():mixed {throw new Exception("hook");}} try{json_encode(new C);}catch(Exception $e){echo $e->getMessage(),"\n";}
--EXPECT--
hook
