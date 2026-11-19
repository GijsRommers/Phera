--TEST--
A reference taken from $this holds the object; nothing can rebind $this
--FILE--
<?php
// $this where a reference is taken: PHP hands over the object itself; nothing can rebind $this.
function byRef(&$x) { $before = get_class($x); $x = 'changed'; return $before; }
function byRefKeep(&$x) { return $x; }
class Target {
    public $p = 1;
    function inArray() { $a = array(&$this); return $a[0] === $this; }
    function inArrayKeyed() { $a = ['k' => &$this, 'n' => 2]; $a['k']->p = 5; return [$a['k'] === $this, $this->p]; }
    function passed() { $r = byRef($this); return [$r, $this instanceof Target, get_class($this)]; }
    function passedKeep() { return byRefKeep($this) === $this; }
    function hook() { return call_user_func_array(fn ($o) => get_class($o), array(&$this)); }
    function hookNamed() { return call_user_func_array('byRefKeep', array(&$this)) === $this; }
    function closureUse() { $f = function () { return array(&$this); }; return $f()[0] === $this; }
    function nested() { $a = [[&$this]]; return $a[0][0] === $this; }
    function sortCb() { $list = [3, 1, 2]; usort($list, array(&$this, 'cmp')); return $list; }
    function cmp($a, $b) { return $a <=> $b; }
    function staticVar() { static $keep; $keep = [&$this]; return count($keep); }
    function propRef() { $r = &$this->p; $r = 9; return $this->p; }
    function listRef() { [$a] = [&$this]; return $a === $this; }
}
$t = new Target;
foreach (['inArray', 'inArrayKeyed', 'passed', 'passedKeep', 'hook', 'hookNamed', 'closureUse', 'nested', 'sortCb', 'staticVar', 'propRef', 'listRef'] as $m) {
    echo str_pad($m, 14), json_encode($t->$m()), "\n";
}
--EXPECT--
inArray       true
inArrayKeyed  [true,5]
passed        ["Target",true,"Target"]
passedKeep    true
hook          "Target"
hookNamed     true
closureUse    true
nested        true
sortCb        [1,2,3]
staticVar     1
propRef       9
listRef       true
