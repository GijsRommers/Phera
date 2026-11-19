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
string(30) "Syntax error near location 1:2"
Syntax error near location 1:2
bool(false)
string(30) "Syntax error near location 1:4"
Syntax error near location 1:4
bool(false)
string(30) "Syntax error near location 2:6"
Syntax error near location 2:6
bool(false)
string(30) "Syntax error near location 1:6"
Syntax error near location 1:6
bool(false)
string(30) "Syntax error near location 1:3"
Syntax error near location 1:3
bool(false)
string(30) "Syntax error near location 1:3"
Syntax error near location 1:3
bool(false)
string(30) "Syntax error near location 1:1"
Syntax error near location 1:1
bool(false)
string(74) "Malformed UTF-8 characters, possibly incorrectly encoded near location 1:1"
Malformed UTF-8 characters, possibly incorrectly encoded near location 1:1
bool(false)
string(12) "Syntax error"
Syntax error
No error
