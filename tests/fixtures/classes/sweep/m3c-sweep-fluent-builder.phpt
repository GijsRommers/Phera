--TEST--
Application class sweep: fluent-builder
--FILE--
<?php
class Query {
public array $filters=[];
function where(
string $name,
 int $value)
: static {
$this->filters[$name]=$value;
return $this;
}
 function total(
)
: int {
$total=0;
foreach(
$this->filters as $value)
{
$total+=$value;
}
return $total;
}
}
 class NamedQuery extends Query {
}
 $query=(new NamedQuery)->where('active',
1)->where('limit',
10);
echo get_class($query),
"\n",
$query->total(),
"\n";

--EXPECT--
NamedQuery
11
