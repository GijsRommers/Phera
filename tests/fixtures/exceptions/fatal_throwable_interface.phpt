--TEST--
Exceptions: fatal_throwable_interface
--FILE--
<?php
try{class Invalid implements Throwable{}}catch(Throwable){echo "bad";}
--EXPECTF--

Fatal error: Class Invalid cannot implement interface Throwable, extend Exception or Error instead in %s on line 2
