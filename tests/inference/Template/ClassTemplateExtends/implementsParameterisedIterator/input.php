<?php
/**
 * @implements \IteratorAggregate<int,\stdClass>
 */
class SelectEntries implements \IteratorAggregate
{
    public function getIterator(): SelectIterator {
        return new SelectIterator();
    }
}

/**
 * @implements \Iterator<int,\stdClass>
 * @psalm-suppress UnimplementedInterfaceMethod
 */
class SelectIterator implements \Iterator
{
}