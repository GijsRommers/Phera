<?php
// Everything a request can reach, changed by some requests and read by all.
// Every fork must print what PHP prints when it runs this file from the top
// (PHP-FPM's model: each request boots again); MUTATE (or ?mutate=1) chooses
// the requests that change it all after the snapshot.

namespace Phera {
    if (!\function_exists('Phera\snapshot')) {
        function snapshot(): bool { return true; }
    }
}

namespace {
    final class Point { public function __construct(public int $x, public int $y) {} }

    class Registry
    {
        public static int $count = 0;
        public static array $log = [];
        public array $items = [];
        const FAVOURITE = Suit::Hearts;

        public function names(): \Closure { return fn () => $this->items; }
    }

    enum Suit: string { case Hearts = 'H'; case Spades = 'S'; }

    const ORIGIN = new Point(0, 0);

    function tick(): int { static $calls = 0; return ++$calls; }

    $registry = new Registry;
    $registry->items = ['boot'];
    $alias = &$registry->items;
    $data = ['nested' => ['point' => new Point(1, 2)], 'list' => [1, 2, 3]];
    $list = &$data['list'];
    $counter = function () use (&$data) { static $n = 0; return ++$n; };
    $names = $registry->names();
    $stack = new SplStack;
    $stack->push(new Point(5, 5));
    $storage = new SplObjectStorage;
    $storage[$registry] = 'boot';
    $wrapped = new ArrayObject(['point' => new Point(7, 7)]);
    $queue = new SplPriorityQueue;
    $queue->insert('boot', 1);
    $fixed = SplFixedArray::fromArray([new Point(8, 8)]);
    $weak = new WeakMap;
    $weak[$registry] = new Point(9, 9);
    $reference = WeakReference::create($registry);
    $when = new DateTimeImmutable('2026-01-02 03:04:05', new DateTimeZone('UTC'));
    $suit = Suit::Hearts;
    ini_set('precision', '10');
    set_error_handler(function (int $level, string $message) { echo "boot handler: $message\n"; return true; });
    register_shutdown_function(function () { echo "boot shutdown\n"; });

    \Phera\snapshot();

    $mutate = ($_GET['mutate'] ?? getenv('MUTATE')) === '1';
    if ($mutate) {
        Registry::$count = 99;
        Registry::$log[] = 'changed';
        $origin = ORIGIN;
        $origin->x = 42;
        tick();
        tick();
        $registry->items[] = 'property';
        $alias[] = 'alias';
        $data['nested']['point']->x = 100;
        $list[] = 4;
        $counter();
        $counter();
        $stack->top()->x = 55;
        $stack->push(new Point(6, 6));
        $storage[$registry] = 'changed';
        $wrapped['point']->x = 77;
        $wrapped['added'] = true;
        $queue->insert('changed', 2);
        $fixed[0]->x = 88;
        $weak[$registry]->x = 99;
        $suit = Suit::Spades;
        ini_set('precision', '5');
        set_error_handler(function (int $level, string $message) { echo "request handler: $message\n"; return true; });
        register_shutdown_function(function () { echo "request shutdown\n"; });
    }
    usleep(2000); // other requests on the worker run here

    echo json_encode([
        'count' => Registry::$count,
        'log' => Registry::$log,
        'origin' => ORIGIN->x,
        'favourite' => Registry::FAVOURITE === Suit::Hearts,
        'tick' => tick(),
        'items' => $registry->items,
        'names' => $names(),
        'point' => $data['nested']['point']->x,
        'list' => $data['list'],
        'counter' => $counter(),
        'stack' => count($stack) . ':' . $stack->bottom()->x,
        'storage' => $storage[$registry],
        'wrapped' => $wrapped['point']->x . ':' . count($wrapped),
        'queue' => count($queue) . ':' . $queue->top(),
        'fixed' => $fixed[0]->x,
        'weak' => $weak[$registry]->x . ':' . count($weak),
        'reference' => $reference->get() === $registry,
        'when' => $when->format(DATE_ATOM),
        'suit' => $suit === Suit::Hearts ? 'hearts' : 'spades',
        'case' => Suit::from('H') === Suit::Hearts,
        // A new object takes the next free id, as after a boot in PHP-FPM.
        'id' => spl_object_id(new Point(0, 0)),
    ]), "\n";
    echo 1 / 3, "\n";
    trigger_error('probe');
}
