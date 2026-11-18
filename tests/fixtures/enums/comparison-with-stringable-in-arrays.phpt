--TEST--
Array comparison uses enum identity and Stringable conversion in the same walk
--FILE--
<?php
enum State { case Ready; case Waiting; }
$label = new class {
    public function __toString(): string { echo "cast\n"; return 'label'; }
};
var_dump([State::Ready, $label] == [State::Ready, 'label']);
var_dump([State::Ready, $label] == [State::Waiting, 'label']);
--EXPECT--
cast
bool(true)
bool(false)
