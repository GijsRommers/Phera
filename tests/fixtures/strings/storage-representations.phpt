--TEST--
Interned and shared strings compare and index arrays by the same bytes
--FILE--
<?php
class StringProperties {
    private string $privateName = 'private';
    protected string $protectedName = 'protected';
    public string $publicName = 'public';
    public function properties(): array {
        return [$this->privateName, $this->protectedName, $this->publicName];
    }
}
$literal = 'alpha';
$edited = $literal;
$edited[0] = 'A';
$appended = $literal;
$appended .= '-beta';
$computed = implode('', ['al', 'pha']);
var_dump($literal, $edited, $appended, $computed === $literal, $edited === $literal);

$map = ['alpha' => 11, '09' => 12, '9' => 13, '-0' => 14, '9223372036854775808' => 15];
foreach (range(100, 120) as $key) {
    $map[$key] = $key;
}
var_dump($map[$computed], $map['alpha'], $map['09'], $map[9], $map['-0'], $map['9223372036854775808']);
unset($map[$computed]);
var_dump(isset($map['alpha']), $map['9'], get_class(new StringProperties), (new StringProperties)->properties());
--EXPECT--
string(5) "alpha"
string(5) "Alpha"
string(10) "alpha-beta"
bool(true)
bool(false)
int(11)
int(11)
int(12)
int(13)
int(14)
int(15)
bool(false)
int(13)
string(16) "StringProperties"
array(3) {
  [0]=>
  string(7) "private"
  [1]=>
  string(9) "protected"
  [2]=>
  string(6) "public"
}
