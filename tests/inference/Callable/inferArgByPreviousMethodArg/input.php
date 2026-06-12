<?php
final class ArrayList
{
    /**
     * @template A
     * @template B
     * @template C
     * @param list<A> $list
     * @param callable(A): B $first
     * @param callable(B): C $second
     * @return list<C>
     */
    public function map(array $list, callable $first, callable $second): array
    {
        throw new RuntimeException("never");
    }
}
$result = (new ArrayList())->map([1, 2, 3], fn($i) => ["num" => $i], fn($i) => ["object" => $i]);
