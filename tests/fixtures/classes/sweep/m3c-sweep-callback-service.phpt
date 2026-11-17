--TEST--
Application class sweep: callback-service
--FILE--
<?php
class Formatter {
function __construct(
public string $prefix)
{
}
function format(
string $text)
: string {
return $this->prefix.$text;
}
}
$formatter=new Formatter(
'item:')
;
$callback=[$formatter,
'format'];
$items=['one',
'two',
'three'];
foreach(
$items as $item)
{
echo call_user_func(
$callback,
$item),
"\n";
}
echo $callback('four'),
"\n";
echo is_callable($callback)?'callable':'bad';

--EXPECT--
item:one
item:two
item:three
item:four
callable
