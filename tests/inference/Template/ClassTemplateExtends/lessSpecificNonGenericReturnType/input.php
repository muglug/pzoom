<?php
/**
 * @template-implements IteratorAggregate<int, int>
 */
class Bar implements IteratorAggregate {
    public function getIterator() : Traversable {
        yield from range(0, 100);
    }
}

$bat = new Bar();

foreach ($bat as $num) {}