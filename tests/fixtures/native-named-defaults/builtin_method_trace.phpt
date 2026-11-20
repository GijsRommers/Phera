--TEST--
Native method failures capture their own method and arguments
--SKIPIF--
<?php if (!extension_loaded('pdo_sqlite')) die('skip PDO SQLite required'); ?>
--FILE--
<?php
ini_set('zend.exception_ignore_args', '0');

function showFrame(Closure $operation): void {
    try {
        $operation();
    } catch (Throwable $error) {
        $frame = $error->getTrace()[0];
        $firstLine = explode("\n", $error->getTraceAsString(), 2)[0];
        echo $frame['class'], $frame['type'], $frame['function'], '/',
            count($frame['args']), ' | ', explode('): ', $firstLine, 2)[1], "\n";
    }
}

showFrame(fn() => Closure::fromCallable('nope'));
showFrame(fn() => (new PDO('sqlite::memory:'))->query('select nope from nowhere'));
showFrame(fn() => (new SplObjectStorage())->offsetGet(new stdClass));
showFrame(fn() => new LimitIterator(new ArrayIterator([1]), -1));
showFrame(fn() => new ReflectionClassConstant(stdClass::class, 'MISSING'));
showFrame(fn() => new ReflectionMethod(stdClass::class, 'missing'));
showFrame(fn() => ReflectionMethod::createFromMethodName('no_colon'));
showFrame(fn() => new ReflectionParameter('strlen', 'missing'));
showFrame(fn() => new ReflectionProperty(stdClass::class, 'missing'));
--EXPECT--
Closure::fromCallable/1 | Closure::fromCallable('nope')
PDO->query/1 | PDO->query('select nope fro...')
SplObjectStorage->offsetGet/1 | SplObjectStorage->offsetGet(Object(stdClass))
LimitIterator->__construct/2 | LimitIterator->__construct(Object(ArrayIterator), -1)
ReflectionClassConstant->__construct/2 | ReflectionClassConstant->__construct('stdClass', 'MISSING')
ReflectionMethod->__construct/2 | ReflectionMethod->__construct('stdClass', 'missing')
ReflectionMethod::createFromMethodName/1 | ReflectionMethod::createFromMethodName('no_colon')
ReflectionParameter->__construct/2 | ReflectionParameter->__construct('strlen', 'missing')
ReflectionProperty->__construct/2 | ReflectionProperty->__construct('stdClass', 'missing')
