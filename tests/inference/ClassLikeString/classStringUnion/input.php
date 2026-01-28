<?php
class Foo
{
    /** @var class-string<TypeOne>|class-string<TypeTwo> */
    public ?string $bar = null;
    /** @var class-string<TypeOne|TypeTwo> */
    public ?string $baz = null;
}

class TypeOne {}

class TypeTwo {}

$foo = new Foo;
$foo->bar = TypeOne::class;
$foo->bar = TypeOne::class;
$foo->baz = TypeTwo::class;
$foo->baz = TypeTwo::class;
