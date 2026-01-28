<?php
enum Foo: int
{
    case Foo = 2;
    case Bar = 3;
}

/** @param value-of<Foo> $arg */
function foobar(int $arg): void
{
    /** @psalm-check-type-exact $arg = 2|3 */;
}

/** @var Foo */
$foo = Foo::Foo;
foobar($foo->value);
                
