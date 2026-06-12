<?php
/**
 * @param array<string, string>|array<int, float> $arg
 * @psalm-assert-if-false array<string, string> $arg
 * @psalm-assert-if-true array<int, float> $arg
 */
function is_b(array $arg): bool { return isset($arg[0]); }

/** @param array<string, string>|array<int, float> $foo */
function f(array $foo): void {
    if (!is_b($foo)) {
        $x = $foo;
        echo count($x);
    } else {
        $y = $foo;
        echo count($y);
    }
}
