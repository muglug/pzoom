<?php
/**
 * @param array{hello: string}|array{world: string} $arg
 * @return bool
 *
 * @psalm-suppress InvalidReturnType
 *
 * @psalm-assert-if-false array{hello: string} $arg
 * @psalm-assert-if-true array{world: string} $arg
 */
function is_array_a_or_b($arg) {}
/**
 * @param array{hello: string} $arg
 * @return void
 */
function takesAnArrayA($arg) {}
/**
 * @param array{world: string} $arg
 * @return void
 */
function takesAnArrayB($arg) {}
/**
 * @var array{hello: string}|array{world: string} $foo
 */
$foo;
if (!is_array_a_or_b($foo)) {
    takesAnArrayA($foo);
} else {
    takesAnArrayB($foo);
}
