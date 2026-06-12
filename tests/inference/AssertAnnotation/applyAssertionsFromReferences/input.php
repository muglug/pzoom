<?php
/** @var string|null */
$foo = "";
$bar = &$foo;

if (assertNotNull($bar)) {
    requiresString($foo);
}

/**
 * @param mixed $foo
 * @psalm-assert-if-true !null $foo
 */
function assertNotNull($foo): bool
{
    return $foo !== null;
}

function requiresString(string $_str): void {}
