--TEST--
Echo releases temporary owners created by its operand
--FILE--
<?php
class Text { function __toString() { return 'text'; } }
$o = new stdClass;
try { echo (string) $o; } catch (Error $e) { echo $e->getMessage(), "\n"; }
$o = new Text;
try { throw new Error('next'); } catch (Error $e) {}
var_dump(new Text);
--EXPECT--
Object of class stdClass could not be converted to string
object(Text)#2 (0) {
}
