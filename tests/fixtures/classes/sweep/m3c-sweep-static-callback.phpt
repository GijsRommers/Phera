--TEST--
Application class sweep: static-callback
--FILE--
<?php
class TextNormalizer {
static function normalize(
string $text)
: string {
return '['.$text.']';
}
}
$callbacks
=['TextNormalizer::normalize',
['TextNormalizer',
'normalize']];
foreach(
$callbacks as $callback)
{
echo call_user_func_array(
$callback,
['one'])
,
"\n";
echo $callback(
'two')
,
"\n";
echo is_callable(
$callback)
?'yes':'no';
echo "\n";
}

--EXPECT--
[one]
[two]
yes
[one]
[two]
yes
