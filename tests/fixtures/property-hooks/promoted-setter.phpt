--TEST--
Constructor promotion calls the setter before entering the constructor body
--FILE--
<?php
class C {
    public function __construct(public int $length = 3 { set => $value * 2; }) {
        echo $this->length, "\n";
    }
}
new C;
new C(5);
--EXPECT--
6
10
