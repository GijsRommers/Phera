--TEST--
Goto resolves labels inside a getter frame
--FILE--
<?php
class Row {
    public int $value {
        get {
            goto result;
            echo "unreachable\n";
            result:
            return 7;
        }
    }
}
echo (new Row)->value, "\n";
--EXPECT--
7
