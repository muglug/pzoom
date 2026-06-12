<?php
/**
 * @template A
 */
final class ArrayList
{
    /**
     * @template B
     *
     * @param callable(A): B $ab
     * @return ArrayList<B>
     */
    public function map(callable $ab) { throw new RuntimeException("???"); }
}

/**
 * @return ArrayList<int>
 */
function getList() { throw new RuntimeException("???"); }

/**
 * @template T
 * @return Closure(T): T
 */
function id() { throw new RuntimeException("???"); }

$result = getList()->map(id());
