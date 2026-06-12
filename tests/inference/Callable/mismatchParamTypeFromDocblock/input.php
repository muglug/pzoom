<?php
/**
 * @template A
 */
final class ArrayList
{
    /**
     * @template B
     * @param Closure(A): B $effect
     * @return ArrayList<B>
     */
    public function map(Closure $effect): ArrayList
    {
        throw new RuntimeException("???");
    }
}

/**
 * @template T
 * @template B
 *
 * @param ArrayList<T> $list
 * @return ArrayList<array{T}>
 */
function genericContext(ArrayList $list): ArrayList
{
    return $list->map(
        /** @param B $_a */
        function ($_a) {
            return [$_a];
        }
    );
}
