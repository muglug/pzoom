<?php
final class Ops
{
    /**
     * @template T
     * @return Closure(list<T>): T
     */
    public function flatten() { throw new RuntimeException("???"); }
}
/**
 * @return list<list<int>>
 */
function getList() { throw new RuntimeException("???"); }
/**
 * @template T
 * @return Closure(list<T>): T
 */
function flatten() { throw new RuntimeException("???"); }
/**
 * @template A
 * @template B
 *
 * @param list<A> $_a
 * @param callable(A): B $_ab
 * @return list<B>
 */
function map(array $_a, callable $_ab) { throw new RuntimeException("???"); }

$ops = new Ops;
$result = map(getList(), $ops->flatten());
