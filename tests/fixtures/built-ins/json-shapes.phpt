--TEST--
JSON encoding and validation preserve flags, errors and callbacks
--FILE--
<?php
function json_probe($value, $flags = 0) {
    var_dump(json_encode($value, $flags, 3), json_last_error());
}
function validate_probe($text, $flags = 0) {
    var_dump(json_validate($text, 3, $flags), json_last_error());
}
class Nested implements JsonSerializable {
    public function jsonSerialize(): mixed {
        echo "serialize\n";
        return ['ascii' => 232, 'null' => null];
    }
}
json_probe(12);
json_probe(PHP_INT_MIN);
json_probe(['ab' => "x/y<z> & ' \" \x00\x7fé"]);
json_probe(['ab' => "x/y<z> & ' \" \x00\x7fé"], JSON_HEX_TAG | JSON_HEX_AMP | JSON_HEX_APOS | JSON_HEX_QUOT | JSON_UNESCAPED_SLASHES);
json_probe([17, new Nested(), 18]);
$n = [1];
$n[] = &$n;
json_probe($n, JSON_PARTIAL_OUTPUT_ON_ERROR);
json_probe(["\xff\xc2"], JSON_INVALID_UTF8_SUBSTITUTE);
validate_probe('{"\u0000":12,"key":"a","key":[null,1e999]}');
validate_probe('{"key":"\uD800"}');
validate_probe('["\uD83D\uDE00"]');
validate_probe("\"\xff\"", JSON_INVALID_UTF8_IGNORE);
validate_probe('[1] trailing');
validate_probe('[[[]]]');
--EXPECT--
string(2) "12"
int(0)
string(20) "-9223372036854775808"
int(0)
string(37) "{"ab":"x\/y<z> & ' \" \u0000\u00e9"}"
int(0)
string(60) "{"ab":"x/y\u003Cz\u003E \u0026 \u0027 \u0022 \u0000\u00e9"}"
int(0)
serialize
string(33) "[17,{"ascii":232,"null":null},18]"
int(0)
string(8) "[1,null]"
int(6)
string(16) "["\ufffd\ufffd"]"
int(0)
bool(true)
int(0)
bool(false)
int(10)
bool(true)
int(0)
bool(true)
int(0)
bool(false)
int(4)
bool(false)
int(1)
