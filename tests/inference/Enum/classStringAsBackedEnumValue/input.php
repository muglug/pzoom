<?php
class Foo {}

enum FooEnum: string {
    case Foo = Foo::class;
}

/**
 * @param class-string $s
 */
function noop(string $s): string
{
    return $s;
}

$foo = FooEnum::Foo->value;
noop($foo);
noop(FooEnum::Foo->value);
