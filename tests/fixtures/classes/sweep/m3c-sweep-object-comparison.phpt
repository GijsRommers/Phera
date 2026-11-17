--TEST--
Application class sweep: object-comparison
--FILE--
<?php
class Point {
function __construct(
public int $x,
public int $y)
{
}
}
$first=new Point(
1,
2)
;
$equal=new Point(
1,
2)
;
$different=new Point(
2,
3)
;
$alias=$first;
foreach(
[$equal,
$different,
$alias] as $candidate)
{
echo $first==$candidate?'equal':'different';
echo '/';
echo $first===$candidate?'same':'distinct';
echo "\n";
}

--EXPECT--
equal/distinct
different/distinct
equal/same
