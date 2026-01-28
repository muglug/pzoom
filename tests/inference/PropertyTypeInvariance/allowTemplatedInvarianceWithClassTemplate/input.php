<?php
abstract class Item {}
class Foo extends Item {}

/** @template T */
class Collection {}

/** @template TItem of Item */
abstract class ItemCollection
{
    /** @var Collection<TItem>|null */
    protected $items;
}

/** @extends ItemCollection<Foo> */
class FooCollection extends ItemCollection
{
    /** @var Collection<Foo>|null */
    protected $items;
}
