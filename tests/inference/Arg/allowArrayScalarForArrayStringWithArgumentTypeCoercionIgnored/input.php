<?php declare(strict_types=1);
/** @param array<string> $arr */
function foo(array $arr) : void {}

/** @return array<int, scalar> */
function bar() : array {
    return [];
}

/** @psalm-suppress ArgumentTypeCoercion */
foo(bar());
