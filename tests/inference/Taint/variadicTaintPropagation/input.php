<?php

/**
 * @psalm-pure
 *
 * @param string|int|float $args
 *
 * @psalm-flow ($format, $args) -> return
 */
function variadic_test(string $format, ...$args) : string {
}

echo variadic_test('', '', $_GET['taint'], '');
