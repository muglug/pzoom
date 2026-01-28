<?php
/**
 * @template-implements IteratorAggregate<int, int>
 */
class SomeIterator implements IteratorAggregate
{
    function getIterator()
    {
        yield 1;
    }
}

/** @param \IteratorAggregate<mixed, int> $i */
function takesIteratorOfInts(\IteratorAggregate $i) : void {
    foreach ($i as $j) {
        echo $j;
    }
}

takesIteratorOfInts(new SomeIterator());