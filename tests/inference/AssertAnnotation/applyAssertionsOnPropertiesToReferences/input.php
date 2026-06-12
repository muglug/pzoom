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

if (assertBarNotNull($foo)) {
    requiresString($bar->bar);
}

function requiresString(string $_str): void {}
