--TEST--
phptoken-typed-properties
--FILE--
<?php
$token = new PhpToken(T_STRING, 'a', pos: 4);
var_dump($token->line, $token->pos);
try { $token->id = []; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
unset($token->text);
try { echo $token->text; } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
int(-1)
int(4)
Cannot assign array to property PhpToken::$id of type int
Typed property PhpToken::$text must not be accessed before initialization
