$numbers = [
    '0', '-0.00', '1', '-1', '2', '3', '-7', '12.300', '-1.23456', '.1', '-.005',
    '9999999999999999999999999999.99', '', '+', '-.', "12\0x", "5.4\0\xff",
    ' ', '1e2', '2.4.5', "9\xff", true, false, 42, 2.5, null, [],
];
$counter = 0;
foreach (['bcadd', 'bcsub', 'bcmul', 'bcdiv', 'bcmod', 'bcdivmod', 'bccomp'] as $fn) {
    foreach ($numbers as $left) {
        foreach ($numbers as $right) {
            foreach ([null, 0, 4] as $scale) {
                run($counter++, fn() => $fn($left, $right, $scale));
            }
        }
    }
    run($counter++, fn() => $fn('1', '2', -1));
    run($counter++, fn() => $fn('1', '2', 2147483648));
    run($counter++, fn() => $fn('1'));
    run($counter++, fn() => $fn('1', '2', 1, 2));
}
foreach ($numbers as $num) {
    run($counter++, fn() => bcfloor($num));
    run($counter++, fn() => bcceil($num));
    foreach ([null, 0, 2, 6, -1] as $scale) {
        run($counter++, fn() => bcsqrt($num, $scale));
        foreach (['-3', '0', '1.000', '2', '7', '-0.5', '99999999999999999999999'] as $exp) {
            run($counter++, fn() => bcpow($num, $exp, $scale));
            foreach (['0', '1', '-5', '7.0', '0.3'] as $mod) {
                run($counter++, fn() => bcpowmod($num, $exp, $mod, $scale));
            }
        }
    }
    foreach (RoundingMode::cases() as $mode) {
        foreach ([-4, -1, 0, 1, 5] as $precision) {
            run($counter++, fn() => bcround($num, $precision, $mode));
        }
    }
}
run($counter++, fn() => bcround('1.5'));
run($counter++, fn() => bcround('1.5', 0, 1));
run($counter++, fn() => bcround('1.5', PHP_INT_MAX));
run($counter++, fn() => bcround('1.5', PHP_INT_MIN));
run($counter++, fn() => [bcscale(), bcscale(3), bcadd('1','2'), bcscale(0)]);
run($counter++, fn() => [ini_set('bcmath.scale', '0x4'), bcscale(), bcdiv(1, 4), bcscale(0)]);
run($counter++, fn() => bcscale(-1));
foreach (['-0.000', '0.125', '-1.600', '2.0', '9', ' 1', true, []] as $input) {
    run($counter++, fn() => new BcMath\Number($input));
    foreach (['add', 'sub', 'mul', 'div', 'mod', 'divmod', 'pow', 'compare'] as $method) {
        foreach (['0.00', '1', '2.01', '-3', true, null, [], 'x'] as $operand) {
            run($counter++, fn() => (new BcMath\Number($input))->$method($operand));
        }
    }
    run($counter++, fn() => (new BcMath\Number($input))->powmod(2, 13));
    run($counter++, fn() => (new BcMath\Number($input))->sqrt());
    run($counter++, fn() => (new BcMath\Number($input))->floor());
    run($counter++, fn() => (new BcMath\Number($input))->ceil());
    run($counter++, fn() => (new BcMath\Number($input))->round());
    run($counter++, fn() => (string) new BcMath\Number($input));
    run($counter++, fn() => serialize(new BcMath\Number($input)));
    run($counter++, fn() => unserialize(serialize(new BcMath\Number($input))));
}
$n = new BcMath\Number('3.00');
foreach ([0, '2.0', '3.0', 'x', 2.9, [], null, new BcMath\Number('1.1')] as $x) {
    run($counter++, fn() => $x + $n);
    run($counter++, fn() => $n - $x);
    run($counter++, fn() => $x * $n);
    run($counter++, fn() => $n / $x);
    run($counter++, fn() => $n % $x);
    run($counter++, fn() => $n ** $x);
    run($counter++, fn() => [$n == $x, $n < $x, $n > $x, $x <=> $n]);
}
run($counter++, function () use ($n) { $n->value = '4'; });
run($counter++, function () use ($n) { unset($n->scale); });
run($counter++, function () use ($n) { $n->other = 0; });
run($counter++, fn() => clone $n);
