//! Legacy SPL payloads use the class and depth policy of the enclosing decode.

use std::path::Path;

use phera_extension_api::testing::scripts::{Comparison, Runtime};

#[test]
fn legacy_spl_payloads_inherit_the_decoder_policy() {
    let comparison = Comparison::for_binary("spl-policy", Path::new(env!("CARGO_BIN_EXE_phera")));
    let script = comparison.artifacts.join("policy.php");
    std::fs::write(
        &script,
        r#"<?php
class Sample { public $number = 7; }
$storage = new SplObjectStorage();
$storage[new Sample()] = [new Sample()];
$list = new SplQueue();
$list->push([new Sample()]);
$array = new RecursiveArrayIterator([new Sample()]);
foreach ([$storage, $list, $array] as $container) {
    $class = $container::class;
    $payload = $container->serialize();
    $wire = 'C:' . strlen($class) . ':"' . $class . '":' . strlen($payload) . ':{' . $payload . '}';
    foreach ([true, false, [$class], [Sample::class], [$class, Sample::class]] as $classes) {
        foreach ([1, 2, 3] as $depth) {
            $options = ['allowed_classes' => $classes, 'max_depth' => $depth];
            foreach ([$wire, serialize($container), 'a:1:{i:0;' . $wire . '}'] as $data) {
                try {
                    var_dump(unserialize($data, $options));
                } catch (Throwable $e) {
                    echo $e::class, ': ', $e->getMessage(), "
";
                }
            }
        }
    }
}
$inner = serialize(new ArrayObject([new Sample()]));
$payload = 'x:i:0;a:1:{i:0;' . $inner . '};m:a:0:{}';
$wire = 'C:11:"ArrayObject":' . strlen($payload) . ':{' . $payload . '}';
var_dump(unserialize($wire, ['allowed_classes' => [ArrayObject::class]]));
class Wrapper implements Serializable {
    public $contents;
    public function serialize(): string { return serialize($this->contents); }
    public function unserialize(string $data): void { $this->contents = unserialize($data); }
    public function __serialize(): array { return [$this->contents]; }
    public function __unserialize(array $data): void { $this->contents = $data[0]; }
}
$payload = serialize(new Sample());
$wire = 'C:7:"Wrapper":' . strlen($payload) . ':{' . $payload . '}';
var_dump(unserialize($wire, ['allowed_classes' => [Wrapper::class]]));

function c(string $name, string $payload): string {
    return 'C:' . strlen($name) . ':"' . $name . '":' . strlen($payload) . ':{' . $payload . '}';
}
function wrap(string $name, string $value): string {
    $payload = match ($name) {
        ArrayObject::class, ArrayIterator::class, RecursiveArrayIterator::class => 'x:i:0;a:1:{i:0;' . $value . '};m:a:0:{}',
        SplDoublyLinkedList::class, SplQueue::class, SplStack::class => 'i:0;:' . $value,
        SplObjectStorage::class => 'x:i:1;O:8:"stdClass":0:{},' . $value . ';m:a:0:{}',
        Wrapper::class => $value,
    };
    return c($name, $payload);
}
$types = [ArrayObject::class, ArrayIterator::class, RecursiveArrayIterator::class, SplDoublyLinkedList::class, SplQueue::class, SplStack::class, SplObjectStorage::class];
$leaf = serialize(new Sample());
foreach ($types as $outer) {
    foreach ([...$types, Wrapper::class] as $inner) {
        for ($depth = 1, $wire = $leaf; $depth <= 3; $depth++) {
            $wire = wrap($inner, $wire);
            $data = wrap($outer, $wire);
            foreach ([true, false, [$outer], [$inner], [$outer, $inner, stdClass::class], [$outer, $inner, stdClass::class, Sample::class]] as $allowed) {
                foreach ([1, 2, 3] as $limit) {
                    echo $outer, '|', $inner, '|', $depth, '|', json_encode($allowed), '|', $limit, "\n";
                    try {
                        var_dump(unserialize($data, ['allowed_classes' => $allowed, 'max_depth' => $limit]));
                    } catch (Throwable $error) {
                        echo $error::class, ': ', $error->getMessage(), "\n";
                    }
                }
            }
        }
    }
}

$storage = c('SplDoublyLinkedList', 'i:0;:O:8:"stdClass":0:{}');
$parent = c('ArrayObject', 'x:i:0;a:3:{i:0;' . $storage . 'i:1;r:6;i:2;r:4;};m:a:0:{}');
function checkObjectIdentity($data, $asArray) {
    try {
        $items = unserialize($data, ['allowed_classes' => [ArrayObject::class, SplDoublyLinkedList::class, stdClass::class]]);
        $parent = $asArray ? $items[0] : $items;
        var_dump($parent[0] === $parent[2], $parent[0][0] === $parent[1]);
        if ($asArray) { var_dump($parent[0][0] === $items[1]); }
    } catch (Throwable $error) {
        echo $error->getMessage(), "\n";
    }
}
checkObjectIdentity($parent, false);
$parent = c('ArrayObject', 'x:i:0;a:3:{i:0;' . $storage . 'i:1;r:7;i:2;r:5;};m:a:0:{}');
checkObjectIdentity('a:2:{i:0;' . $parent . 'i:1;r:7;}', true);

class WakeupOrder {
    public function __wakeup(): void { echo "wakeup\n"; }
}
class LegacyOrder extends Wrapper {
    public function unserialize(string $data): void { echo "legacy\n"; }
}
$nested = wrap(ArrayObject::class, wrap(SplDoublyLinkedList::class, serialize(new WakeupOrder())));
unserialize('a:2:{i:0;' . $nested . 'i:1;' . c(LegacyOrder::class, '') . '}');

"#,
    )
    .expect("write the oracle script");
    let run = |runtime| {
        comparison
            .command(runtime, &script)
            .output()
            .expect("run the oracle script")
    };
    let mut php = run(Runtime::Php);
    let mut phera = run(Runtime::Phera);
    // var_dump prints process-local object handles after each class name.
    for output in [&mut php, &mut phera] {
        let text = String::from_utf8_lossy(&output.stdout);
        output.stdout = text
            .split_inclusive('\n')
            .map(|line| {
                if line.trim_start().starts_with("object(")
                    && let Some((class, rest)) = line.split_once(")#")
                {
                    return format!(
                        "{class})#<ID>{}",
                        rest.trim_start_matches(|c: char| c.is_ascii_digit())
                    );
                }
                line.to_owned()
            })
            .collect::<String>()
            .into_bytes();
    }
    comparison
        .check("policy", &php, &phera)
        .unwrap_or_else(|error| panic!("{error}: {}", comparison.artifacts.display()));
}
