--TEST--
Enums: self::Case and self::CONST in methods, match arms and a backing expression
--FILE--
<?php
enum Suit: string {
    const LABEL = 'suit';
    case Hearts = self::LABEL . '-h';
    case Spades = 's';
    public function label(): string { return self::LABEL; }
    public function caseSelf(): self { return self::Hearts; }
    public function describe(): string {
        return match ($this) { self::Hearts => 'H', self::Spades => 'S' };
    }
}
echo Suit::Hearts->value, "\n";
echo Suit::Spades->label(), "\n";
echo Suit::Spades->caseSelf()->name, "\n";
echo Suit::Hearts->describe(), ' ', Suit::Spades->describe(), "\n";
--EXPECT--
suit-h
suit
Hearts
H S
