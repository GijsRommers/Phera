--TEST--
Attribute arguments resolve in the declaring scope and evaluate on demand
--FILE--
<?php
namespace Example;
use Attribute as Marker;

#[Marker(Marker::TARGET_CLASS | Marker::IS_REPEATABLE)]
class Route {
    public function __construct(public string $name, public object $options) {}
}

class Options {
    public function __construct(public int $version) { echo "options\n"; }
}

#[Route('first', options: new Options(self::VERSION)), Route('second', new Options(2))]
/** Route configuration. */
class Controller {
    private const VERSION = 7;
}

spl_autoload_register(function ($name) { echo "autoload: $name\n"; });
$class = new \ReflectionClass(Controller::class);
echo $class->getDocComment(), "\n";
foreach ($class->getAttributes(Route::class) as $attribute) {
    echo $attribute->getName(), " ", $attribute->getTarget(), " ", (int) $attribute->isRepeated(), "\n";
    $object = $attribute->newInstance();
    echo $object->name, " ", $object->options->version, "\n";
}
--EXPECT--
/** Route configuration. */
Example\Route 1 1
options
first 7
Example\Route 1 1
options
second 2
