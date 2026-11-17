--TEST--
invoke exception
--FILE--
<?php
class Fail{function __invoke(){throw new RuntimeException("invoke");}}try{call_user_func(new Fail);}catch(RuntimeException $e){echo $e->getMessage(),"
";}
--EXPECT--
invoke
