--TEST--
Application class sweep: recursive-tree
--FILE--
<?php
class Tree {
public array $children=[];
function __construct(
public int $value)
{
}
 function add(
Tree $child)
: static {
$this->children[]=$child;
return $this;
}
function sum(
)
: int {
$sum=$this->value;
foreach(
$this->children as $child){
$sum+=$child->sum();
}
return $sum;
}
}
 $root=new Tree(1);
$branch=(new Tree(2))->add(new Tree(3));
$root->add($branch)->add(new Tree(4));
echo $root->sum(),
"\n",
$branch->sum(),
"\n";

--EXPECT--
10
5
