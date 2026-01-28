<?php
/**
 * @implements Traversable<int, 1>
 * @implements IteratorAggregate<int, 1>
 */
final class C implements Traversable, IteratorAggregate {
    public function getIterator() {
        yield 1;
    }
}
