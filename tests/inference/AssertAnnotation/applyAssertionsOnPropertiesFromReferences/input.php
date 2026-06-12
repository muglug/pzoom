<?php
class Foo
{
    public ?string $bar = null;
}

/**
 * @psalm-assert-if-true !null $foo->bar
 */
function assertBarNotNull(Foo $foo): bool
{
    return $foo->bar !== null;
}

$foo = new Foo();
$bar = &$foo;

if (assertBarNotNull($bar)) {
    requiresString($foo->bar);
}

function requiresString(string $_str): void {}
