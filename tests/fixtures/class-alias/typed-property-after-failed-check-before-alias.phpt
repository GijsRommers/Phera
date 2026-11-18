--TEST--
Typed property after failed check before alias
--FILE--
<?php
class Original {} class Holder { public Another $value; } $h=new Holder; try { $h->value=new Original; } catch(TypeError $e) { echo "before\n"; } class_alias("Original","Another"); $h->value=new Original; echo get_class($h->value);
--EXPECT--
before
Original