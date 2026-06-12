<?php
/**
 * @return list<int>
 */
function getList() { throw new RuntimeException("???"); }

/**
 * @template T
 * @return Closure(T): T
 */
function id() { throw new RuntimeException("???"); }

/**
 * @template A
 * @template B
 *
 * @param list<A> $_items
 * @param callable(A): B $_ab
 * @return list<B>
 */
function map(array $_items, callable $_ab) { throw new RuntimeException("???"); }

$result = map(getList(), id());
