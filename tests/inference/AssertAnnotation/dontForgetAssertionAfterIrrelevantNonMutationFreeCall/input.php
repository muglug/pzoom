<?php
class Foo
{
    public ?string $bar = null;

    public function nonMutationFree(): void {}
}

/**
 * @psalm-assert-if-true !null $foo->bar
 */
function assertBarNotNull(Foo $foo): bool
{
    return $foo->bar !== null;
}

$foo = new Foo();

if (assertBarNotNull($foo)) {
    $foo->nonMutationFree();
    requiresString($foo->bar);
}

function requiresString(string $_str): void {}
