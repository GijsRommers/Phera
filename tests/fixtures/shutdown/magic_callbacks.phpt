--TEST--
Shutdown callbacks resolve __invoke, __call and __callStatic and retain their arguments
--FILE--
<?php
class ShutdownHandler {
    public function __construct(private string $label) {}

    public function __invoke(string $first, int $second): void {
        echo $this->label, ':invoke ', $first, ' ', $second, "\n";
    }

    public function __call(string $name, array $arguments): void {
        echo $this->label, ':', $name, ' ', implode(' ', $arguments), "\n";
        if ($name === 'missing') {
            register_shutdown_function([$this, 'later'], 'last');
        }
    }

    public static function __callStatic(string $name, array $arguments): void {
        echo $name, ' ', implode(' ', $arguments), "\n";
    }
}
$handler = new ShutdownHandler('bound');
register_shutdown_function($handler, 'one', 2);
register_shutdown_function([$handler, 'missing'], 'three', 4);
register_shutdown_function([ShutdownHandler::class, 'missingStatic'], 'five', 6);
unset($handler);
echo "body\n";
--EXPECT--
body
bound:invoke one 2
bound:missing three 4
missingStatic five 6
bound:later last
