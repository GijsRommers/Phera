--TEST--
JSON errors identify the token and preserve the previous error on throws
--FILE--
<?php
foreach (["{", "[1,]", "{\n \"é\" 0}", "true false", "[1.]", "[2e]", "\"abc", "\"\xff\"", ''] as $input) {
    var_dump(json_validate($input), json_last_error_msg());
    try { json_decode($input, flags: JSON_THROW_ON_ERROR); }
    catch (JsonException $error) { echo $error->getMessage(), PHP_EOL; }
}
json_decode('null');
echo json_last_error_msg(), PHP_EOL;
--EXPECT--
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(12) "Syntax error"
Syntax error
bool(false)
string(53) "Control character error, possibly incorrectly encoded"
Control character error, possibly incorrectly encoded
bool(false)
string(56) "Malformed UTF-8 characters, possibly incorrectly encoded"
Malformed UTF-8 characters, possibly incorrectly encoded
bool(false)
string(12) "Syntax error"
Syntax error
No error
