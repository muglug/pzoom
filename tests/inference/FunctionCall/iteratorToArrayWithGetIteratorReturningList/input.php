<?php
/**
 * @implements IteratorAggregate<int, string>
 */
class C implements IteratorAggregate {
    /**
     * @return Traversable<int, string>
     */
    public function getIterator() {
        yield 1 => "1";
    }
}
$a = iterator_to_array(new C, false);
