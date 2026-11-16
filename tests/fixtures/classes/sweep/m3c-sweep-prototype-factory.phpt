--TEST--
Application class sweep: prototype-factory
--FILE--
<?php
class Prototype {
function __construct(
public int $id)
{
}
function copy(
int $id)
: static {
$copy=clone $this;
$copy->id=$id;
return $copy;
}
}
class Specialized extends Prototype {
}
$prototype=new Specialized(
1)
;
$copies=[$prototype->copy(
2)
,
$prototype->copy(
3)];
foreach($copies as $copy){
echo get_class($copy),
'=',
$copy->id,
"\n";
}
echo $prototype->id;

--EXPECT--
Specialized=2
Specialized=3
1
