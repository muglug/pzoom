<?php
/**
 * @template E
 * @template-extends \IteratorAggregate<int, E>
 */
interface Collection extends \IteratorAggregate
{
    /**
     * @return \Iterator<int,E>
     */
    public function getIterator(): \Iterator;
}

/**
 * @template-implements Collection<string>
 */
abstract class Set implements Collection {
    public function forEach(callable $action): void {
        $i = $this->getIterator();
        foreach ($this as $bar) {
            $action($bar);
        }
    }
}