--TEST--
Application class sweep: runtime-factory
--FILE--
<?php
interface Transport {
function send(
string $text)
: string;
}
 class Mail implements Transport {
function send(
string $text)
: string {
return 'mail:'.$text;
}
}
 class Sms implements Transport {
function send(
string $text)
: string {
return 'sms:'.$text;
}
}
 class Factory {
static function make(string $name): Transport {
return new $name;
}
}
 foreach(['Mail',
'Sms'] as $name){
$transport=Factory::make($name);
echo $transport->send('hello'),
"\n";
}

--EXPECT--
mail:hello
sms:hello
