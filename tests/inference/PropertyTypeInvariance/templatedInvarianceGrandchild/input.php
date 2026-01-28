<?php
abstract class Item {}
class Foo extends Item {}
class Bar extends Foo {}

/** @template T of Item */
abstract class ItemCollection
{
    /** @var list<T> */
    protected $items = [];
}

/**
 * @template T of Foo
 * @extends ItemCollection<T>
 */
class FooCollection extends ItemCollection
{
    /** @var list<T> */
    protected $items = [];
}

/** @extends FooCollection<Bar> */
class BarCollection extends FooCollection
{
    /** @var list<Bar> */
    protected $items = [];
}
