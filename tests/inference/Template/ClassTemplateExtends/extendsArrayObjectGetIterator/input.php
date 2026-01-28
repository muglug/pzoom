<?php
class Obj {}

/**
 * @template T1
 * @template-extends ArrayObject<int, T1>
 */
class Collection extends ArrayObject {}

/**
 * @template T2 as Obj
 * @template-extends Collection<T2>
 */
class Collection2 extends Collection {
    /**
     * called to get the collection ready when we go to loop through it
     *
     * @return \ArrayIterator<int, T2>
     */
    public function getIterator() {
        return parent::getIterator();
    }
}
