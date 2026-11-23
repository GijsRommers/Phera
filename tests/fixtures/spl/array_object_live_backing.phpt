--TEST--
ArrayObject visits live array and object-backed entries during modification
--FILE--
<?php
function row($stage,$key,$value) { echo $stage,' ',$key,'=',json_encode($value),"\n"; }
$items = new ArrayObject(['a'=>1,'b'=>2,'c'=>3,'d'=>4]);
foreach ($items as $key => $value) {
    row('array',$key,$value);
    if ($key === 'a') {
        unset($items['b']);
        $items['c'] = 30;
        $items['e'] = 5;
    }
    if ($key === 'c') unset($items['c']);
}
foreach ($items as $key => $value) {
    row('replace',$key,$value);
    if ($key === 'a') $items->exchangeArray(['x'=>10,'y'=>20]);
}
class DataRow {
    private $private = 'private';
    protected $protected = 'protected';
    public int $a = 1;
    public $b = 2;
    public $c = 3;
}
$row = new DataRow;
$items = new ArrayObject($row, ArrayObject::ARRAY_AS_PROPS);
row('copy', '', $items->getArrayCopy());
row('keys', '', array_keys($items->getArrayCopy()));
foreach ($items as $key => $value) {
    row('object',$key,$value);
    if ($key === 'a') {
        $row->b = 20;
        $items->c = 30;
        $items->z = 40;
    }
}
row('wrapped','',$row);
try {
    $items['a'] = 'x';
} catch (TypeError $e) {
    echo $e->getMessage(),"\n";
}
row('typed', 'a', $row->a);
--EXPECTF--
array a=1
array c=30
array e=5
replace a=1
replace y=20

Deprecated: ArrayObject::__construct(): Using an object as a backing array for ArrayObject is deprecated, as it allows violating class constraints and invariants in %s on line 25
copy ={"\u0000DataRow\u0000private":"private","\u0000*\u0000protected":"protected","a":1,"b":2,"c":3}
keys =["\u0000DataRow\u0000private","\u0000*\u0000protected","a","b","c"]
object a=1
object b=20
object c=30
object z=40
wrapped ={"a":1,"b":20,"c":30,"z":40}
typed a="x"
