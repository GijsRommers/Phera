--TEST--
Dynamic property names select the lexical declaration in small and wide classes
--FILE--
<?php
class ParentRow {
    private $item = 'parent';
    protected $label = 'protected';
    public function parentRead($name) { return $this->$name; }
}
class SmallRow extends ParentRow {
    private $item = 'child';
    public $Item = 'case';
    public function childRead($name) { return $this->$name; }
}
class WideRow extends SmallRow {
    public $a, $b, $c, $d, $e, $f, $g, $h, $i, $j;
    private static $hidden = 1;
}
foreach ([new SmallRow, new WideRow] as $row) {
    echo $row->parentRead('item'), '/', $row->childRead('item'), '/',
        $row->childRead('Item'), '/', $row->parentRead('label'), "\n";
    $name = 'Item';
    $ref = &$row->$name;
    $ref = 'changed';
    echo $row->childRead($name), "\n";
    var_dump(isset($row->missing));
}
$row = new WideRow;
$name = 'hidden';
try { var_dump($row->$name); } catch (Error $e) { echo $e->getMessage(), "\n"; }

class SortRow {
    public $z = 1, $a = 2;
}
$sorted = new SortRow;
(new ArrayObject($sorted))->ksort();
$name = 'z';
$sorted->$name += 5;
echo implode(',', array_keys((array) $sorted)), '/', $sorted->$name, '/', $sorted->a, "\n";
--EXPECTF--
parent/child/case/protected
changed
bool(false)
parent/child/case/protected
changed
bool(false)
Cannot access private property WideRow::$hidden

Deprecated: ArrayObject::__construct(): Using an object as a backing array for ArrayObject is deprecated, as it allows violating class constraints and invariants in %s on line %d
a,z/6/2
