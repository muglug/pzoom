<?php
declare(strict_types = 1);

namespace App;

interface FooInterface {}

/**
 * @method \IteratorAggregate<int, FooInterface> getAll():\IteratorAggregate
 */
class Foo
{
    private \IteratorAggregate $items;

    /**
     */
    public function getAll(): \IteratorAggregate
    {
        return $this->items;
    }

    public function __construct(\IteratorAggregate $foos)
    {
        $this->items = $foos;
    }
}

/**
 * @method \IteratorAggregate<int, FooInterface> getAll():\IteratorAggregate
 */
class Bar
{
    private \IteratorAggregate $items;

    /**
     */
    public function getAll(): \IteratorAggregate
    {
        return $this->items;
    }

    public function __construct(\IteratorAggregate $foos)
    {
        $this->items = $foos;
    }
}
