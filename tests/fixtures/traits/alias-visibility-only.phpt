--TEST--
traits: alias visibility only
--FILE--
<?php
trait T{function m(){return 3;}} class C{use T {m as private;} function check(){return $this->m();}} echo (new C)->check();
--EXPECT--
3
