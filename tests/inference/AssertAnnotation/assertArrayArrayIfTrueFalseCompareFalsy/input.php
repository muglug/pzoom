<?php
/**
 * @param array<string, string>|array<int, float> $arg
 * @return bool
 *
 * @psalm-suppress InvalidReturnType
 *
 * @psalm-assert-if-false array<string, string> $arg
 * @psalm-assert-if-true array<int, float> $arg
 */
function is_array_a_or_b($arg) {}
/**
 * @param array<string, string> $arg
 * @return void
 */
function takesAnArrayA($arg) {}
/**
 * @param array<int, float> $arg
 * @return void
 */
function takesAnArrayB($arg) {}
/**
 * @var array<string, string>|array<int, float> $foo
 */
$foo;
if (!is_array_a_or_b($foo)) {
    takesAnArrayA($foo);
} else {
    takesAnArrayB($foo);
}
