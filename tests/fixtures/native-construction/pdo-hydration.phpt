--TEST--
PDO row hydration invokes non-public constructors with the declaring scope
--FILE--
<?php
class PrivateRow {
    public $value = 'empty';
    private function __construct($suffix) { $this->append($suffix); }
    private function append($suffix) { $this->value .= $suffix; }
}
class ProtectedRow {
    public $value = 'initial';
    protected function __construct($suffix = '?') {
        echo 'constructor saw ', $this->value, "\n";
        $this->value .= $suffix;
    }
}
$db = new PDO('sqlite::memory:');
$query = "SELECT 'hello' AS value";
echo $db->query($query)->fetchObject(PrivateRow::class, ['suffix' => '!'])->value, "\n";
echo $db->query($query)->fetchAll(PDO::FETCH_CLASS, PrivateRow::class, ['.'])[0]->value, "\n";
echo $db->query($query, PDO::FETCH_CLASS, ProtectedRow::class, ['#'])->fetch()->value, "\n";
$rows = $db->query($query);
$rows->setFetchMode(PDO::FETCH_CLASS, ProtectedRow::class, ['=']);
echo $rows->fetchAll()[0]->value, "\n";
echo $db->query($query)->fetchAll(PDO::FETCH_CLASS | PDO::FETCH_PROPS_LATE, ProtectedRow::class)[0]->value, "\n";
try { new PrivateRow('!'); } catch (Error $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
hello!
hello.
constructor saw hello
hello#
constructor saw hello
hello=
constructor saw initial
hello
Call to private PrivateRow::__construct() from global scope
