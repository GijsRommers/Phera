--TEST--
Application class sweep: singleton
--FILE--
<?php
class Settings {
private static ?Settings $instance=null;
public array $values=[];
private function __construct(
)
{
}
 static function instance(
)
: Settings {
if(
self::$instance===null)
{
self::$instance=new self;
}
return self::$instance;
}
function set(
$key,
$value)
{
$this->values[$key]=$value;
}
}
 $first=Settings::instance();
$first->set('mode',
'test');
$second=Settings::instance();
echo $first===$second?'same':'different';
echo $second->values['mode'];

--EXPECT--
sametest
