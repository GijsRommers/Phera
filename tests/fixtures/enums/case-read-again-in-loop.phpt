--TEST--
enums: a case read again and again is the same object, with from, cases and constants between
--FILE--
<?php
enum Unit { case X; case Y; const FIRST = self::X; public static function pick(): self { return self::Y; } }
enum Backed: string { case A = "a"; case B = "b"; const DEFAULT = self::B; public function other(): self { return $this === self::A ? self::B : self::A; } }
function reads(int $times): array {
    $same = 0; $ids = [];
    for ($i = 0; $i < $times; $i++) {
        $same += Unit::X === Unit::FIRST ? 1 : 0;
        $same += Unit::pick() === Unit::Y ? 1 : 0;
        $same += Backed::A === Backed::from("a") ? 1 : 0;
        $same += Backed::tryFrom("b") === Backed::DEFAULT ? 1 : 0;
        $same += Backed::A->other() === Backed::cases()[1] ? 1 : 0;
        $same += match (Backed::from($i % 2 ? "a" : "b")) { Backed::A => 1, Backed::B => 1 };
        $ids[spl_object_id(Backed::A)] = true;
        $ids[spl_object_id(Unit::cases()[0])] = true;
    }
    return [$same, count($ids)];
}
var_dump(reads(3), reads(50));
var_dump(Backed::A, Unit::Y);
--EXPECT--
array(2) {
  [0]=>
  int(18)
  [1]=>
  int(2)
}
array(2) {
  [0]=>
  int(300)
  [1]=>
  int(2)
}
enum(Backed::A)
enum(Unit::Y)
