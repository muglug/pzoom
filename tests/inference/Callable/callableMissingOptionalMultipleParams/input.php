<?php
/**
 * @param callable(string, string, string, string=):bool $arg
 * @return void
 */
function foo($arg) {}

function bar(string $a, string $b, string $c): bool {}

foo("bar");
