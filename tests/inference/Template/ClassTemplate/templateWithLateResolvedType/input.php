<?php
/**
 * @template A of Enum::TYPE_*
 */
class Foo {}

class Enum
{
    const TYPE_ONE = 1;
    const TYPE_TWO = 2;
}

/** @var Foo<Enum::TYPE_ONE> $foo */
$foo = new Foo();