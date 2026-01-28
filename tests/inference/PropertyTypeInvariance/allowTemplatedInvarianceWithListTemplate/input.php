<?php
abstract class Item {}
class Foo extends Item {}

/** @template TItem of Item */
abstract class ItemCollection
{
    /** @var list<TItem> */
    protected $items = [];
}

/** @extends ItemCollection<Foo> */
class FooCollection extends ItemCollection
{
    /** @var list<Foo> */
    protected $items = [];
}
