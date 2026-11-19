--TEST--
Native custom debug views precede ordinary uninitialized slot handling
--FILE--
<?php
class Fixed extends SplFixedArray { public $x; public int $y = 3; public string $late; }
$fixed = new Fixed(2); $fixed[0] = 'a';
var_dump($fixed, unserialize(serialize($fixed)));
class Moment extends DateTimeImmutable { public function __construct(string $datetime = 'now', public ?stdClass $note = null) { parent::__construct($datetime); } }
var_dump(new Moment('2022-12-22T11:26:00Z', new stdClass));
class Plain { public int $set = 1; public ?Plain $unset; }
var_dump(new Plain);

--EXPECT--
object(Fixed)#1 (4) {
  [0]=>
  string(1) "a"
  [1]=>
  NULL
  ["x"]=>
  NULL
  ["y"]=>
  int(3)
}
object(Fixed)#2 (4) {
  [0]=>
  string(1) "a"
  [1]=>
  NULL
  ["x"]=>
  NULL
  ["y"]=>
  int(3)
}
object(Moment)#2 (4) {
  ["note"]=>
  object(stdClass)#3 (0) {
  }
  ["date"]=>
  string(26) "2022-12-22 11:26:00.000000"
  ["timezone_type"]=>
  int(2)
  ["timezone"]=>
  string(1) "Z"
}
object(Plain)#2 (1) {
  ["set"]=>
  int(1)
  ["unset"]=>
  uninitialized(?Plain)
}
