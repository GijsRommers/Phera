$numbers = [
    '-18446744073709551616', '123456789012345678901234567890',
    '-100', '0', 1, 2, 3, 4, 7, 12, 22, 12345, '010', '0x10000', '0b101', '0o77',
    '-0xF', '1 2', "3\0x", '+12', '3.5', "9\xff", '', false, true, 2.5, null, [],
];
$counter = 0;
foreach (['gmp_add', 'gmp_sub', 'gmp_mul', 'gmp_mod', 'gmp_gcd', 'gmp_gcdext', 'gmp_lcm',
          'gmp_invert', 'gmp_jacobi', 'gmp_legendre', 'gmp_kronecker', 'gmp_cmp',
          'gmp_and', 'gmp_or', 'gmp_xor', 'gmp_hamdist'] as $fn) {
    foreach ($numbers as $left) {
        foreach ($numbers as $right) {
            run($counter++, fn() => $fn($left, $right));
        }
    }
}
foreach ($numbers as $num) {
    foreach (['gmp_neg', 'gmp_abs', 'gmp_com', 'gmp_sign', 'gmp_sqrt', 'gmp_sqrtrem',
              'gmp_perfect_square', 'gmp_perfect_power', 'gmp_popcount',
              'gmp_intval', 'gmp_nextprime'] as $fn) {
        run($counter++, fn() => $fn($num));
    }
    foreach ([-37, -36, -16, -2, 0, 1, 2, 8, 10, 16, 36, 37, 62, 63, PHP_INT_MAX] as $base) {
        run($counter++, fn() => gmp_init($num, $base));
        run($counter++, fn() => gmp_strval($num, $base));
    }
    foreach ([-1, 0, 1, 2, 3, 5, 63, 64, PHP_INT_MAX] as $index) {
        run($counter++, fn() => gmp_testbit($num, $index));
        run($counter++, fn() => gmp_scan0($num, $index));
        run($counter++, fn() => gmp_scan1($num, $index));
        run($counter++, fn() => gmp_root($num, $index));
        run($counter++, fn() => gmp_rootrem($num, $index));
    }
    foreach ([-9, -2, 0, 2, 9] as $divisor) {
        foreach ([0, 1, 2, 3] as $rounding) {
            run($counter++, fn() => gmp_div_qr($num, $divisor, $rounding));
            run($counter++, fn() => gmp_div_q($num, $divisor, $rounding));
            run($counter++, fn() => gmp_div_r($num, $divisor, $rounding));
            run($counter++, fn() => gmp_div($num, $divisor, $rounding));
        }
        run($counter++, fn() => gmp_divexact(gmp_mul($num, $divisor), $divisor));
        run($counter++, fn() => gmp_pow($num, $divisor));
        run($counter++, fn() => gmp_binomial($num, $divisor));
        run($counter++, fn() => gmp_powm($num, $divisor, -11));
    }
    foreach ([-1, 0, 1, 2, 4, PHP_INT_MAX] as $word) {
        foreach ([0, 1, 2, 4, 5, 6, 8, 9, 10, 17, 18, 3, 24] as $flags) {
            run($counter++, fn() => gmp_export($num, $word, $flags));
        }
    }
}
foreach (['', "\0", "\x01\xff", "\x01\x02\x03\x04", "\xff\0\0\x01\x42"] as $bytes) {
    foreach ([0, 1, 2, 3, 4, PHP_INT_MAX] as $word) {
        foreach ([0, 1, 2, 4, 5, 6, 8, 9, 10, 17, 18, 3, 24] as $flags) {
            run($counter++, fn() => gmp_import($bytes, $word, $flags));
        }
    }
}
foreach ([-1, 0, 1, 2, 10, 100, 256, [], 'bad'] as $n) {
    run($counter++, fn() => gmp_fact($n));
}
foreach (['-7', 0, 1, 2, 3, 4, 5, 1373653, 2147483647, '2305843009213693951',
          '170141183460469231731687303715884105727', []] as $n) {
    foreach ([-5, 0, 1, 10, 25, 50] as $reps) {
        run($counter++, fn() => gmp_prob_prime($n, $reps));
    }
}
foreach ([0, 1, -1, '12345678901234567890123456789'] as $seed) {
    run($counter++, fn() => gmp_random_seed($seed));
    foreach ([1, 7, 8, 31, 32, 33, 64, 65, 127, 128, 4096, 0, -1, PHP_INT_MAX] as $bits) {
        run($counter++, fn() => gmp_random_bits($bits));
    }
    foreach ([[-10, 9], [-5, -4], [0, 1], [9, 9], [9, 1], [1, '1000000000000000000000']] as [$min, $max]) {
        run($counter++, fn() => gmp_random_range($min, $max));
    }
}
$n = gmp_init(-13);
foreach ([0, 1, 2, -3, true, '', '6', 'bad', '0x10', 2.5, new GMP(5), [], null] as $x) {
    run($counter++, fn() => $x + $n);
    run($counter++, fn() => $n - $x);
    run($counter++, fn() => $x * $n);
    run($counter++, fn() => $n / $x);
    run($counter++, fn() => $n % $x);
    run($counter++, fn() => $n ** $x);
    run($counter++, fn() => $n << $x);
    run($counter++, fn() => $n >> $x);
    run($counter++, fn() => $n & $x);
    run($counter++, fn() => $x | $n);
    run($counter++, fn() => $n ^ $x);
    run($counter++, fn() => [$n == $x, $n < $x, $n > $x, $x <=> $n]);
}
run($counter++, fn() => [~$n, -$n, +$n, !$n, (bool)new GMP(), (int)$n, (float)$n, (string)$n]);
run($counter++, fn() => [serialize($n), unserialize(serialize($n)), clone $n, $n->__serialize()]);
foreach ([-1, 0, 1, 12, 64, 257, PHP_INT_MAX] as $index) {
    run($counter++, function () use ($n, $index) { gmp_setbit($n, $index); return clone $n; });
    run($counter++, function () use ($n, $index) { gmp_clrbit($n, $index); return clone $n; });
}
