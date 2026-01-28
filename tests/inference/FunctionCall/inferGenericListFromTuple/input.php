<?php
/**
 * @template A
 * @param list<A> $list
 * @return list<A>
 */
function testList(array $list): array { return $list; }
/**
 * @template A
 * @param non-empty-list<A> $list
 * @return non-empty-list<A>
 */
function testNonEmptyList(array $list): array { return $list; }
/**
 * @template A of list<mixed>
 * @param A $list
 * @return A
 */
function testGenericList(array $list): array { return $list; }
$list = testList([1, 2, 3]);
$nonEmptyList = testNonEmptyList([1, 2, 3]);
$genericList = testGenericList([1, 2, 3]);
