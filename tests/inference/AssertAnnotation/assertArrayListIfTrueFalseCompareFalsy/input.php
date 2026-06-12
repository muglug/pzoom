<?php
/**
 * @param array<string, string|int|float>|list<string> $arg
 * @return bool
 *
 * @psalm-assert-if-false array<string, string|int|float> $arg
 * @psalm-assert-if-true list<string> $arg
 */
function is_array_or_list($arg) {
    if (array_values($arg) === $arg) {
        return true;
    }
    return false;
}
/**
 * @param list<string> $arg
 * @return void
 */
function takesAList($arg) {}
/**
 * @param array<string, string|int|float> $arg
 * @return void
 */
function takesAnArray($arg) {}
/**
 * @var array<string, string|int|float>|list<string> $foo
 */
$foo;
if (!is_array_or_list($foo)) {
    takesAnArray($foo);
} else {
    takesAList($foo);
}
