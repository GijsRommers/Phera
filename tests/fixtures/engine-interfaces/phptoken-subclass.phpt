--TEST--
phptoken-subclass
--FILE--
<?php
class ChildToken extends PhpToken {
    public int $extra=123;
    public function lowered(): string { return strtolower($this->text); }
}
foreach (ChildToken::tokenize("<?PHP FOO") as $token) {
    echo get_class($token), " ", $token->lowered(), " ", $token->extra, "\n";
}
--EXPECT--
ChildToken <?php  123
ChildToken foo 123
