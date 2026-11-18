--TEST--
anonymous-classes: parent call
--FILE--
<?php
class P { function name() { return "parent"; } } echo (new class extends P { function name() { return parent::name(); } })->name();
--EXPECT--
parent
