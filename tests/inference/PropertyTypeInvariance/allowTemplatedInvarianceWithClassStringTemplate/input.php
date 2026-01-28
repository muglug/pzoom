<?php
abstract class Item {}
class Foo extends Item {}

/** @template T of Item */
abstract class ItemType
{
    /** @var class-string<T>|null */
    protected $type;
}

/** @extends ItemType<Foo> */
class FooTypes extends ItemType
{
    /** @var class-string<Foo>|null */
    protected $type;
}
