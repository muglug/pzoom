<?php
/**
 * @param list<string>|list<int> $arg
 * @return bool
 *
 * @psalm-suppress InvalidReturnType
 *
 * @psalm-assert-if-false list<string> $arg
 * @psalm-assert-if-true list<int> $arg
 */
function is_list_string_or_int($arg) {}
/**
 * @param list<string> $arg
 * @return void
 */
function takesAListString($arg) {}
/**
 * @param list<int> $arg
 * @return void
 */
function takesAListInt($arg) {}
/**
 * @var list<string>|list<int> $foo
 */
$foo;
if (!is_list_string_or_int($foo)) {
    takesAListString($foo);
} else {
    takesAListInt($foo);
}
