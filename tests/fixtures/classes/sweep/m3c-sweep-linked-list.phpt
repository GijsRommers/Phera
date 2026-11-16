--TEST--
Application class sweep: linked-list
--FILE--
<?php
class Node {
public ?Node $next=null;
function __construct(
public int $value)
{
}
function sum(
)
: int {
return $this->value+(
$this->next===null?0:$this->next->sum(
)
);
}
function append(Node $node): void {
if($this->next===null){
$this->next=$node;
}
else{
$this->next->append($node);
}
}
}
 $head=new Node(3);
$head->append(new Node(4));
$head->append(new Node(5));
echo $head->sum(),
"\n",
$head->next->sum(),
"\n";

--EXPECT--
12
9
