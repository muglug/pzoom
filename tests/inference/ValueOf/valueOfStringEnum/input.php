<?php
enum Foo: string
{
    case Foo = "foo";
    case Bar = "bar";
}

/** @param value-of<Foo> $arg */
function foobar(string $arg): void
{
    /** @psalm-check-type-exact $arg = "foo"|"bar" */;
}

/** @var Foo */
$foo = Foo::Foo;
foobar($foo->value);
                
